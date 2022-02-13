type ICloudError = icloud::error::Error;

#[derive(Debug)]
pub enum Error {
   ICloudError(ICloudError),
   IOError(std::io::Error),
   JSONError(serde_json::Error),
   DriveNotAvailable,
   RuntimeNotAvailable
}

impl From<ICloudError> for Error {

    fn from(error: ICloudError) -> Error {
        Error::ICloudError(error)
    }

}

impl From<std::io::Error> for Error {

    fn from(error: std::io::Error) -> Error {
        Error::IOError(error)
    }

}

impl From<serde_json::Error> for Error {

    fn from(error: serde_json::Error) -> Error {
        Error::JSONError(error)
    }

}
