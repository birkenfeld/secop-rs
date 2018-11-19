//! Demo modules.

pub mod cryo;

use std::thread;
use serde_json::Value;

use crate::config::ModuleConfig;
use crate::module::{Module, ModInternals};


fn inner_run<T: Module + Send + 'static>(cfg: ModuleConfig, internals: ModInternals) -> Value {
    let module = T::create(cfg, internals);
    let descriptive = module.describe();
    thread::spawn(|| module.run());
    descriptive
}


pub fn run_module(cfg: ModuleConfig, internals: ModInternals) -> Result<Value, String> {
    match &*cfg.class {
        "Cryo" => Ok(inner_run::<cryo::Cryo>(cfg, internals)),
        _ => Err(format!("No such module class: {}", cfg.class))
    }
}
