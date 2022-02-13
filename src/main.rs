extern crate icloud_fuse;

use std::io::{
    stdin, 
    stdout,
    BufReader,
    BufWriter,
    Write
};
use std::path::Path;
use std::fs::File;
use std::sync::Arc;
use tokio::runtime::Runtime;

use icloud::client::Client;
use icloud::session::SessionData;

use icloud_fuse::{
    Error,
    ICloudFilesystem,
    SyncMutex,
    AsyncMutex
};

async fn login_prompt() -> (String, String) {
    print!("Enter username: ");
    stdout().flush().unwrap();
    let mut username = String::new();
    if let Err(msg) = stdin().read_line(&mut username) {
        panic!("{}", msg);
    }
    username.truncate(username.len() - 1);
    print!("Enter password: ");
    stdout().flush().unwrap();
    let mut password = String::new();
    if let Err(msg) = stdin().read_line(&mut password) {
        panic!("{}", msg);
    }
    password.truncate(password.len() - 1);
    (username, password)
}

async fn prompt_2fa() -> String {
    print!("Enter 2FA code: ");
    stdout().flush().unwrap();
    let mut code = String::new();
    if let Err(msg) = stdin().read_line(&mut code) {
        panic!("{}", msg);
    }
    code.truncate(code.len() - 1);
    code
}

async fn authenticate(client: &mut Client) -> Result<(), icloud::error::Error> {

    match client.authenticate().await {
        Err(icloud::error::Error::AuthenticationFailed(_)) | 
            Err(icloud::error::Error::MissingCacheItem(_)) => {
            let (username, password) = login_prompt().await;
            match client.login(username.as_str(), password.as_str()).await {
                Ok(()) => Ok(()),
                Err(err) => match err {
                    icloud::error::Error::Needs2FA => {
                        let code = prompt_2fa().await;
                        client.authenticate_2fa(code.as_str()).await?;
                        Ok(())
                    }
                    _ => Err(err),
                },
            }
        }
        Err(icloud::error::Error::Needs2FA) => {
            let code = prompt_2fa().await;
            client.authenticate_2fa(code.as_str()).await?;
            Ok(())
        }
        Err(err) => {
            println!("{}", err);
            Err(err)
        }
        Ok(()) => Ok(()),
    }
}


pub fn main() -> Result<(), Error> {
        
    let mountpoint = std::env::args_os().nth(1).unwrap();

    let path = Path::new("cache.json");
    let session_data: SessionData = if path.exists() {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader)?
    } else {
        SessionData::new()?
    };

    let runtime = Arc::new(SyncMutex::new(Runtime::new().unwrap()));

    if let Ok(mut client) = Client::new(session_data) {
        let drive = Arc::new(AsyncMutex::new(if let Ok(runtime) = runtime.lock() {
            runtime.block_on(async {
                authenticate(&mut client).await?;
                if let Some(drive) = client.drive().await {
                    Ok(drive)
                } else {
                    Err(Error::DriveNotAvailable)
                }
            })
        } else { 
            Err(Error::RuntimeNotAvailable) 
        }?));

        fuser::mount2(ICloudFilesystem::new(runtime.clone(), drive.clone())?, &mountpoint, &[])?;

        let file = if path.exists() {
            File::options().write(true).open(path)
        } else {
            File::create(path)
        }?;
        let writer = BufWriter::new(file);
        let data = if let Ok(runtime) = runtime.lock() {
            Ok(runtime.block_on(client.save()))
        } else {
            Err(Error::RuntimeNotAvailable)
        }?;
        serde_json::to_writer(writer, &data)?;

    }

    Ok(())
}
