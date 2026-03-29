use std::str::FromStr;

use clap::Args;
use stellar_strkey::DecodeError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("decoding {0:?}: {1}")]
    Decode(String, DecodeError),
}

#[derive(Args, Debug, Clone)]
#[command()]
pub struct Cmd {
    /// Strkey to decode
    #[arg()]
    strkey: String,
}

impl Cmd {
    pub fn run(&self) -> Result<(), Error> {
        let strkey = stellar_strkey::Strkey::from_str(&self.strkey)
            .map_err(|e| Error::Decode(self.strkey.clone(), e))?;
        println!("{strkey:?}");
        Ok(())
    }
}
