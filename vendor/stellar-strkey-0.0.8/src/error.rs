#[derive(thiserror::Error, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum DecodeError {
    // TODO: Add meaningful errors for each problem that can occur.
    #[error("the strkey is invalid")]
    Invalid,
}
