extern crate icloud_fuse;

use std::fs::File;
use std::io::{stdin, stdout, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

use clap::Parser;
use rpassword::read_password_from_tty;
use tokio::runtime::Runtime;

use icloud::client::Client;
use icloud::drive::DriveService;
use icloud::SessionData;

use icloud_fuse::{AsyncMutex, Error, ICloudFilesystem, SyncMutex};
use icloud_fuse::error::ICloudError;

#[derive(Parser)]
struct Args {
    #[clap(
        required = true,
        long = "username",
        short = 'u',
        help = "iCloud username"
        )]
        username: String,

        #[clap(required = true, help = "The directory to mount the iCloud Drive")]
        mountpoint: String,

        #[clap(default_value = "cache.json")]
        cache: String,
}

pub fn main() -> Result<(), Error> {
    let args = Args::parse();

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
        let drive = runtime.lock().map(|runtime| {
            runtime.block_on(client.authenticate()).or_else(|err| {
                match err {
                    ICloudError::AuthenticationFailed(_) |
                        ICloudError::MissingCacheItem(_) => {
                            let password = read_password_from_tty(Some("Password: "))?;
                            runtime.block_on(client.login(args.username.as_str(), password.as_str())).or_else(|err| {
                                match err {
                                    ICloudError::Needs2FA => {
                                        let code = read_password_from_tty(Some("2FA Code: "))?;
                                        runtime.block_on(client.authenticate_2fa(code.as_str()))?;
                                        Ok(())
                                    }, _ => {
                                        Err(err)
                                    }
                                }
                            })
                        },
                    err => Err(err)
                }
            })
        });

        let drive : Arc<AsyncMutex<DriveService>> = Arc::new(AsyncMutex::new(
                runtime.lock().map(|runtime| {
                    runtime.block_on(client.drive()).ok_or(Error::DriveNotAvailable)
                }).map_err(|_| Error::DriveNotAvailable)??));

        fuser::mount2(
            ICloudFilesystem::new(runtime.clone(), drive.clone())?,
            &args.mountpoint,
            &[],
            )?;

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
