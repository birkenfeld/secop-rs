//! Demo modules.

pub mod cryo;

use std::thread;
use serde_json::Value;

use crate::module::{Module, ModInternals};


fn inner_run<T: Module + Send + 'static>(internals: ModInternals) -> Value {
    let module = T::create(internals);
    let descriptive = module.describe();
    thread::spawn(|| module.run());
    descriptive
}


pub fn run_module(internals: ModInternals) -> Result<Value, String> {
    Ok(match &*internals.class() {
        "Cryo" => inner_run::<cryo::Cryo>(internals),
        _ => return Err(format!("No such module class: {}", internals.class()))
    })
}
