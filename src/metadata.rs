use std::collections::{BTreeMap, BTreeSet};
use fuser::{
    FileAttr,
    FileType
};
use async_recursion::async_recursion;
use icloud::drive::{
    DriveNode,
    DriveService
};
use icloud::error::Error;

pub struct Metadata {
    inode: u64,
    parent: Option<u64>,
    node: DriveNode,
    children: BTreeSet<u64>
}

impl Metadata {

    pub fn inode(&self) -> u64 {
        self.inode
    } 

    pub fn name(&self) -> &String {
        self.node.name()
    }

    pub fn kind(&self) -> FileType {
        match self.node {
            DriveNode::Folder(_) => FileType::Directory,
            DriveNode::File(_) => FileType::RegularFile,
        }
    }

    pub fn children(&self) -> &BTreeSet<u64> {
        &self.children
    }

    pub fn parent(&self) -> Option<u64> {
        self.parent
    }

}

impl Into<FileAttr> for &Metadata {

    fn into(self) -> FileAttr {
        match &self.node {
            DriveNode::Folder(_) => FileAttr {
                ino: self.inode,
                blocks: 1,
                blksize: 4096,
                size: 4096,
                atime: self.node.date_created().into(),
                ctime: self.node.date_created().into(),
                crtime: self.node.date_created().into(),
                mtime: self.node.date_created().into(),
                flags: 0,
                uid: 0,
                gid: 0,
                kind: fuser::FileType::Directory,
                nlink: 0,
                rdev: 0,
                perm: 0o400
            }, DriveNode::File(file) => FileAttr {
                ino: self.inode,
                blocks: 1,
                blksize: 4096,
                size: 4096,
                atime: file.last_opened.unwrap().into(),
                ctime: file.date_changed.into(),
                crtime:self.node.date_created().into(),
                mtime: file.date_modified.into(),
                flags: 0,
                uid: 0,
                gid: 0,
                kind: fuser::FileType::RegularFile,
                nlink: 0,
                rdev: 0,
                perm: 0o600
            }
        }
    }

}

pub struct MetadataTable {
    next_inode: u64,
    inodes: BTreeMap<u64, Metadata>
}

#[async_recursion]
async fn update_node_metadata(mut metadata: &mut MetadataTable, node: &DriveNode, parent: Option<u64>) {
    let inode_num = metadata.insert(node, parent);
    if let DriveNode::Folder(folder) = node {
        for item in folder.iter() {
            update_node_metadata(&mut metadata, item, Some(inode_num)).await;
        }
    }
}

impl MetadataTable { 

    pub fn new() -> MetadataTable {
        MetadataTable{
            next_inode: 1,
            inodes: BTreeMap::new()
        }
    }

    fn get_by_id(&self, id: &String) -> Option<&Metadata> {
        for (_, value) in &self.inodes {
            if value.node.id() == id {
                return Some(&value);
            }
        }
        None
    }

    pub async fn update(&mut self, drive: &mut DriveService) -> Result<(), Error> {
        let root = drive.root().await?;
        update_node_metadata(self, &DriveNode::Folder(root), None).await;
        Ok(())
    }


    pub fn insert(&mut self, node: &DriveNode, parent: Option<u64>) -> u64 {
        match self.get_by_id(node.id()) {
            Some(metadata) => metadata.inode,
            None => {
                let inode = self.next_inode;
                let metadata = Metadata {
                    inode: inode,
                    parent: parent,
                    node: (*node).clone(),
                    children: BTreeSet::new()
                };

                if let Some(parent) = parent {
                    if let Some(metadata) = self.inodes.get_mut(&parent) {
                        metadata.children.insert(inode);
                    }
                }

                self.inodes.insert(self.next_inode, metadata); 
                self.next_inode += 1;
                inode
            }
        }
    }

    pub fn get_by_name(&self, name: String, parent: u64) -> Option<&Metadata> {
        for (_, metadata) in &self.inodes {
            if *metadata.name() == name {
                if let Some(node_parent) = metadata.parent()  {
                    if parent == node_parent {
                        return Some(metadata)
                    }
                }
            }
        }
        None
    }

    pub async fn get(&self, inode: &u64) -> Option<&Metadata> {
        self.inodes.get(inode)
    }

}
