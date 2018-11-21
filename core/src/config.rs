// -----------------------------------------------------------------------------
// Rust SECoP playground
//
// This program is free software; you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation; either version 2 of the License, or (at your option) any later
// version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along with
// this program; if not, write to the Free Software Foundation, Inc.,
// 59 Temple Place, Suite 330, Boston, MA  02111-1307  USA
//
// Module authors:
//   Georg Brandl <g.brandl@fz-juelich.de>
//
// -----------------------------------------------------------------------------
//
//! Configuration file handling.

use std::collections::HashSet;
use std::path::Path;
use serde_derive::Deserialize;
use toml;


#[derive(Deserialize)]
pub struct ServerConfig {
    pub description: String,
    #[serde(skip)]
    pub equipment_id: String,
    pub modules: Vec<ModuleConfig>,
}

#[derive(Deserialize, Clone)]
pub struct ModuleConfig {
    pub name: String,
    pub class: String,
    pub description: String,
    pub group: Option<String>,
}


pub fn load_config(filename: impl AsRef<Path>) -> Result<ServerConfig, String> {
    let data = std::fs::read(&filename).map_err(|e| e.to_string())?;
    let mut obj: ServerConfig = toml::from_slice(&data).map_err(|e| e.to_string())?;
    obj.equipment_id = filename.as_ref()
                               .file_stem()
                               .map_or("unknown".into(), |s| s.to_string_lossy().into_owned());

    // TODO: check groups as well
    let mut lc_names = HashSet::new();
    for modcfg in &obj.modules {
        if !lc_names.insert(modcfg.name.to_lowercase()) {
            return Err(format!("module name {} is not unique", modcfg.name))
        }
    }

    Ok(obj)
}
