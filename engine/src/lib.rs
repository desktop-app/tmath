#![forbid(unsafe_code)]

pub mod enums;
pub mod env;
pub mod font;
pub mod glue;
pub mod tables;
pub mod text;

#[cfg(test)]
mod env_tests;
#[cfg(test)]
mod font_tests;
