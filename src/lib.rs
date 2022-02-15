use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use futures::lock::Mutex;
use icloud::drive::{
    DriveService,
    DriveNode
};
use libc::ENOENT;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::runtime::Runtime;
use async_recursion::async_recursion;

pub mod error;
pub mod metadata;
use metadata::MetadataTable;

pub type Error = error::Error;
pub type SyncMutex<T> = std::sync::Mutex<T>;
pub type AsyncMutex<T> = futures::lock::Mutex<T>;

pub struct ICloudFilesystem {
    last_update: SystemTime,
    drive: Arc<AsyncMutex<DriveService>>,
    metadata: Arc<AsyncMutex<MetadataTable>>,
    runtime: Arc<SyncMutex<Runtime>>,
}

#[async_recursion]
async fn update_node_metadata(
    metadata: &mut MetadataTable,
    drive: &mut DriveService,
    node: &DriveNode,
    parent: Option<u64>,
) -> Result<(), Error> {
    let inode_num = metadata.insert(node, parent);
    if let DriveNode::Folder(folder) = node {
        for item in folder.iter() {
            if let DriveNode::Folder(_) = item {
                let node = drive.get_node(item.id()).await?;
                update_node_metadata(metadata, drive, &node, Some(inode_num)).await?;
            } else {
                metadata.insert(item, Some(inode_num));
            }
        }
    }
    Ok(())
}

impl ICloudFilesystem {
    pub fn new(runtime: Arc<SyncMutex<Runtime>>,
               drive: Arc<futures::lock::Mutex<DriveService>>) -> Result<ICloudFilesystem, Error> {
        let mut fs: ICloudFilesystem = ICloudFilesystem {
            last_update: SystemTime::now(),
            drive: drive.clone(),
            metadata: Arc::new(AsyncMutex::new(MetadataTable::new())),
            runtime: runtime,
        };

        fs.update();
        Ok(fs)
    }

    fn update(&mut self) {
        if let Ok(runtime) = self.runtime.lock() {
            let drive = self.drive.clone();
            let metadata = self.metadata.clone();
            runtime.block_on(async move {
                let mut metadata = metadata.lock().await;
                let mut drive = drive.lock().await;
                let root = drive.root().await.unwrap();
                update_node_metadata(&mut metadata, &mut drive, &DriveNode::Folder(root), None).await.unwrap();
            });
        }

    }

}

impl Filesystem for ICloudFilesystem {


    fn lookup(&mut self, _req: &Request, parent: u64, name: &std::ffi::OsStr, reply: ReplyEntry) {
        if let Ok(runtime) = self.runtime.lock() {
            let metadata = self.metadata.clone();
            runtime.block_on(async move {
                let metadata = metadata.lock().await;
                let entry = metadata.get_by_name(String::from(name.to_str().unwrap()), parent);
                if let Some(entry) = entry {
                    reply.entry(&Duration::new(1, 0), &entry.into(), 0);
                } else {
                    reply.error(ENOENT);
                }
            });
           
        } else {
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        if let Ok(runtime) = self.runtime.lock() {
            let metadata = self.metadata.clone();
            runtime.spawn(async move {
                let metadata = metadata.lock().await;
                if let Some(metadata) = metadata.get(&ino).await {
                    reply.attr(&Duration::new(1, 0), &metadata.into());
                } else { 
                    reply.error(ENOENT);
                }
            });
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
        ) {
        reply.error(ENOENT);
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
        ) {

        if let Ok(runtime) = self.runtime.lock() {
            let metadata = self.metadata.clone();
            if let Some(results) = runtime.block_on(async move {
                let metadata = metadata.lock().await;
                if let Some(directory) = metadata.get(&ino).await {
                    let mut children: Vec<(u64, i64, fuser::FileType, String)> = vec![];
                    for (index, value) in directory.children().iter().enumerate().skip(offset as usize) {
                        if let Some(metadata) = metadata.get(value).await {
                            children.push((metadata.inode(), (index + 1) as i64, metadata.kind(), (*metadata.name()).clone())); 
                        }
                    }
                    Some(children)
                } else {
                    None
                }
            }) {
                for (inode, offset, kind, name) in results {
                    reply.add(inode, offset, kind, name);
                }
                reply.ok();
            } else {
                reply.error(ENOENT);
            }
        }
    }
}


