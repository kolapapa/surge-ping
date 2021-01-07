#![allow(dead_code)]
use std::io;

use packet::icmp::Kind;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SurgeError>;

#[derive(Error, Debug)]
pub enum SurgeError {
    #[error("packet parse error")]
    PacketError(#[from] packet::Error),
    #[error("io error")]
    IOError(#[from] io::Error),
    #[error("packet kind error")]
    KindError(Kind),
    #[error("timeout error")]
    Timeout,
}
