#![feature(generic_associated_types)]
#![allow(unstable_name_collisions)]
#![warn(clippy::pedantic)]
#![forbid(unused_must_use)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]

pub use entity::{Create, Delete, Entity, Field, Update};
pub use manager::Manager;

pub mod entity;
pub mod manager;
pub use foil_macros::{patch, patch_opt, selector, Create, Delete, Entity, Update, Value};

println!("test");
