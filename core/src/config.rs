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

use std::path::Path;
use fxhash::{FxHashMap as HashMap, FxHashSet as HashSet};
use serde_derive::{Serialize, Deserialize};
use serde_json::Value;
use toml;


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    None,
    User,
    Advanced,
    Expert,
}

impl Default for Visibility {
    fn default() -> Self { Visibility::User }
}

#[derive(Deserialize, Debug)]
pub struct ServerConfig {
    #[serde(skip)] // provided by us
    pub equipment_id: String,
    pub description: String,
    pub modules: HashMap<String, ModuleConfig>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ModuleConfig {
    pub class: String,
    pub description: String,
    pub group: Option<String>,
    #[serde(default)]
    pub parameters: HashMap<String, Value>,
    #[serde(default)]
    pub visibility: Visibility,
}


pub fn load_config(filename: impl AsRef<Path>) -> Result<ServerConfig, String> {
    let data = std::fs::read(&filename).map_err(|e| e.to_string())?;
    let mut obj: ServerConfig = toml::from_slice(&data).map_err(|e| e.to_string())?;
    obj.equipment_id = filename.as_ref()
                               .file_stem()
                               .map_or("unknown".into(), |s| s.to_string_lossy().into_owned());

    // Check module names and groups for lowercase-uniqueness.
    let mut lc_names = HashSet::default();
    let mut lc_groups = HashSet::default();
    for modcfg in obj.modules.values() {
        if let Some(group) = modcfg.group.as_ref() {
            lc_groups.insert(group.to_string());
        }
    }
    for name in obj.modules.keys() {
        let lc_name = name.to_lowercase();
        if lc_groups.contains(&lc_name) || !lc_names.insert(lc_name) {
            return Err(format!("module name {} is not unique amoung modules and groups", name))
        }
    }

    Ok(obj)
}
