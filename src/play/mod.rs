//! Demo modules.

pub mod cryo;

use std::thread;
use crate::config::ModuleConfig;
use crate::module::{Module, ModInternals};


fn inner_run<T: Module>(cfg: ModuleConfig, internals: ModInternals) {
    thread::spawn(|| T::create(cfg, internals).run());
}


pub fn run_module(cfg: ModuleConfig, internals: ModInternals) -> Result<(), String> {
    match &*cfg.class {
        "Cryo" => inner_run::<cryo::Cryo>(cfg, internals),
        _ => return Err(format!("No such module class: {}", cfg.class))
    }
    Ok(())
}
