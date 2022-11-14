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
//! Derive a SECoP ModuleBase implementation for individual modules.
//!
//! Provides an implementation of the `ModuleBase` trait for a given struct,
//! which contains the custom data required for a module's hardware-facing
//! implementation.
//!
//! Additionally, the struct must have a member called `internals` of type
//! `secop::module::ModInternals`, which represents the basic data and
//! communication interfaces that this framework requires for each module, and a
//! member called `cache` of type `<Struct>ParamCache` (generated by the derive
//! macro), which is a store of all previous parameter values and timestamps.
//!
//! Parameters and commands are added to the module interface using attributes.
//! For example:
//!
//! ```
//! #[derive(ModuleBase)]
//! #[param(name="status", datainfo="StatusType", readonly=True)]
//! #[param(name="value", datainfo="Double()", readonly=True)]
//! #[param(name="target", datainfo="Double()")]
//! #[param(name="speed", datainfo="Double(min=0.0)", default="1.0")]
//! #[command(name="stop", argtype="Null", restype="Null")]
//! struct Motor {
//!     // required by the framework
//!     internals: ModInternals,
//!     cache: MotorParamCache,
//!     // module specific, to talk to the controller
//!     connection: SerialPort,
//! }
//! ```
//!
//! You must afterwards also implement the `Module` trait, which contains all
//! APIs that cannot be derived automatically, and inherent methods that
//! implement the actual reading, writing, and execution.  These have very
//! simple signatures since all data is in terms of Rust types, and has been
//! validated against the SECoP type specification.  For the above example:
//!
//! ```
//! impl Module for Motor {
//!     fn create(internals: ModInternals) -> Self {
//!         // create the serial port, using internals.config to access the
//!         // user configuration of the module
//!         let connection = ...;
//!         Motor { internals, connection }
//!     }
//! }
//!
//! // expected argument types here are determined by the `datatype` selected
//! // in the param/command attribute above
//! impl Motor {
//!     // note, read_ can take &mut self or &self
//!     fn read_value(&mut self) -> Result<f64> { ... }
//!     fn write_target(&mut self, tgt: f64) -> Result<()> { ... }
//!     fn do_stop(&mut self, arg: ()) -> Result<()> { ... }
//! }
//! ```

use std::collections::HashSet;
use proc_macro2::TokenStream;
use syn::{Error, Expr, spanned::Spanned};
use quote::{quote, quote_spanned, format_ident};
use darling::FromMeta;


/// All the possible properties of a parameter.
///
/// Representation of the #[param(...)] attribute.
#[derive(FromMeta, Debug)]
struct SecopParam {
    /// Name of the parameter.
    name: String,
    /// Documentation/description (also transmitted to clients).
    doc: String,
    /// Datatype, from secop_core::types or self-defined.
    datainfo: String,
    /// If true, the parameter cannot be changed from a client.
    readonly: bool,
    /// If true, the parameter is only software-related and does not
    /// need to be propagated to hardware.
    #[darling(default)]
    swonly: bool,
    /// If true, the parameter must be given in the config file.
    /// (Not possible if readonly and !swonly).
    #[darling(default)]
    mandatory: bool,
    /// If given, a default value for the parameter.
    /// Parameters with swonly set *must* have a default.
    #[darling(default)]
    default: Option<String>,
    /// Poll interval, in multiples of the poll interval.
    /// If negative, do not accelerate polling when module is busy.
    /// Parameters with swonly set are not polled.
    #[darling(default)]
    polling: Option<i64>,
    /// The unit of the parameter's value.
    #[darling(default)]
    unit: String,
    /// The group to display the parameter under.
    #[darling(default)]
    group: String,
    /// The visibility of the parameter, can be "none" to not
    /// transmit information about it to clients.
    #[darling(default = "default_visibility")]
    visibility: String,
}

// Can't use the definition of core, since core depends on this crate.
const VISIBILITIES: &[&str] = &["none", "user", "advanced", "expert"];
fn default_visibility() -> String { "user".into() }

/// Representation of the #[command(...)] attribute.
#[derive(FromMeta, Debug)]
struct SecopCommand {
    name: String,
    doc: String,
    argtype: String,
    restype: String,
    #[darling(default)]
    group: String,
    #[darling(default = "default_visibility")]
    visibility: String,
}


/// Parse an attribute (using darling) into the given struct representation.
fn parse_attr<T: FromMeta>(attr: &syn::Attribute) -> Result<T, TokenStream> {
    attr.parse_meta()
        .map_err(|err| format!("invalid param attribute: {}", err))
        .and_then(|meta| T::from_meta(&meta).map_err(|_| "could not parse this attribute".into()))
        .map_err(|e| quote_spanned! { attr.span() => compile_error!(#e); })
}

/// Use instead of panic!() to assign the error to a proper span, if possible.
macro_rules! try_ {
    ($expr:expr) => (
        match $expr {
            Ok(v) => v,
            Err(e) => return TokenStream::from(e.to_compile_error())
        }
    );
    ($expr:expr, $($tt:tt)*) => (
        match $expr {
            Ok(v) => v,
            Err(mut e) => {
                e.combine(Error::new(e.span(), format!($($tt)*)));
                return TokenStream::from(e.to_compile_error())
            }
        }
    );
}

/// Main derive function for ModuleBase.
pub fn derive_module(input: synstructure::Structure) -> TokenStream {
    let mut params = Vec::new();
    let mut commands = Vec::new();

    let name = &input.ast().ident;
    let vis = &input.ast().vis;
    let param_cache_name = format_ident!("{}ParamCache", name);

    // Parse parameter and command attributes on the main struct.
    for attr in &input.ast().attrs {
        if attr.path.segments[0].ident == "param" {
            match parse_attr::<SecopParam>(attr) {
                Ok(param) => params.push((attr.span(), param)),
                Err(err) => return err
            }
        } else if attr.path.segments[0].ident == "command" {
            match parse_attr::<SecopCommand>(attr) {
                Ok(cmd) => commands.push((attr.span(), cmd)),
                Err(err) => return err
            }
        }
    }

    // Check for required members. (TODO: make these functions on Module instead?)
    let mut has_internals = false;
    let mut has_cache = false;
    match &input.ast().data {
        syn::Data::Struct(syn::DataStruct { fields: syn::Fields::Named(fields), .. }) => {
            for field in &fields.named {
                if field.ident.as_ref().unwrap() == "internals" { has_internals = true; }
                if field.ident.as_ref().unwrap() == "cache" { has_cache = true; }
            }
        }
        _ => try_!(Err(Error::new(input.ast().ident.span(),
                                  "derive(ModuleBase) is only possible for a \
                                   struct with named fields")))
    }

    if !has_internals || !has_cache {
        try_!(Err(Error::new(input.ast().ident.span(),
                             format!("struct {} must have \"internals: ModInternals\" and \
                                      \"cache: {}ParamCache\" members", name, name))));
    }

    // We need to check names for uniqueness, after lowercasing.
    // TODO: also check groups which are in the same namespace.
    let mut lc_names = HashSet::new();

    // Prepare snippets of code to generate.
    let mut statics = vec![];
    let mut par_read_arms = vec![];
    let mut par_write_arms = vec![];
    let mut cmd_arms = vec![];
    let mut descriptive = vec![];
    let mut param_cache = vec![];
    let mut poll_busy_params = vec![];
    let mut poll_other_params = vec![];
    let mut activate_updates = vec![];
    let mut init_params_swonly = vec![];
    let mut init_params_write = vec![];
    let mut init_params_read = vec![];

    for (span,
         SecopParam { name, doc, datainfo, readonly, swonly, mandatory, polling,
                      default, unit, group, visibility }) in params {
        let polling = polling.unwrap_or(if swonly { 0 } else { 1 });

        // Check necessary invariants.
        if !lc_names.insert(name.to_lowercase()) {
            try_!(Err(Error::new(span, "param/cmd name is not unique")));
        }
        if !VISIBILITIES.iter().any(|&v| v == visibility) {
            try_!(Err(Error::new(span, "visibility is not an allowed value")));
        }
        if swonly {
            if polling != 0 {
                try_!(Err(Error::new(span, "software-only parameters cannot be polled")));
            }
            if default.is_none() && !mandatory {
                try_!(Err(Error::new(span, "software-only parameters must have a default if not mandatory")));
            }
        } else {
            if default.is_some() && readonly {
                try_!(Err(Error::new(span, "readonly hardware parameters cannot have a default")));
            }
            if mandatory && readonly {
                try_!(Err(Error::new(span, "readonly hardware parameters cannot be mandatory")));
            }
        }

        let name_id = format_ident!("{}", name);
        // The parameter metatype instances are in principle constant, but
        // cannot be `const`, so we use `lazy_static` instead.
        let type_static = format_ident!("PAR_TYPE_{}", name);
        let (type_t, type_expr) = try_!(crate::parse_datainfo(span, &datainfo));
        statics.push(quote! {
            static ref #type_static: #type_t = #type_expr;
        });
        let type_repr = quote! { <#type_t as TypeInfo>::Repr };

        // Populate members of the parameter cache struct.
        param_cache.push(quote! {
            #name_id: secop_core::module::CachedParam<#type_repr>,
        });

        // Generate trampolines for read and write of the parameter.  These
        // methods are expected to be present as inherent methods on the struct.
        // If forgotten, the errors should be pretty clear.
        let read_method = format_ident!("read_{}", name);
        let write_method = format_ident!("write_{}", name);
        let update_method = format_ident!("update_{}", name);

        par_read_arms.push(match swonly {
            false => quote! {
                #name => (|| {
                    let read_value = self.#read_method()?;
                    let (value, time, send) = self.cache.#name_id.update(read_value, &*#type_static)?;
                    if send {
                        self.send_update(#name, value.clone(), time);
                    }
                    Ok((value, time))
                })()
            },
            true => quote! {
                #name => (|| {
                    let value = #type_static.to_json(self.cache.#name_id.clone())?;
                    Ok((value, self.cache.#name_id.time()))
                })()
            },
        });

        par_write_arms.push(match (swonly, readonly) {
            (false, false) => quote! {
                #name => (|| {
                    self.#write_method(#type_static.from_json(&value)?)?;
                    self.read(#name)
                })()
            },
            (true, false) => quote! {
                #name => (|| {
                    // TODO: simplify?
                    let (value, time, send) =
                        self.cache.#name_id.update(#type_static.from_json(&value)?, &*#type_static)?;
                    if send {
                        self.send_update(#name, value.clone(), time);
                        self.#update_method(self.cache.#name_id.clone())?;
                    }
                    Ok(json!([value, {"t": time}]))
                })()
            },
            (_, true)  => quote! {
                #name => Err(Error::new(ErrorKind::ReadOnly, ""))
            },
        });

        // Generate entry for the polling loop.
        if polling != 0 {
            let polling_period = polling.abs() as usize;
            let poll_it = quote! {
                if n % #polling_period == 0 {
                    // TODO: error handling (should send a error_update message)
                    let _ = self.read(#name);
                }
            };
            if polling > 0 {
                poll_busy_params.push(poll_it);
            } else {
                poll_other_params.push(poll_it);
            }
        }

        // Generate entries for the "initial updates" phase of activation.
        activate_updates.push(quote! {
            // TODO: really ignore errors?
            if let Ok(value) = #type_static.to_json(self.cache.#name_id.clone()) {
                res.push(Msg::Update { module: self.name().to_string(),
                                       param: #name.to_string(),
                                       data: json!([value, {"t": self.cache.#name_id.time()}]) });
            }
        });

        // Generate parameter initialization code.
        //
        // This is quite complex since we have multiple sources (defaults from
        // code, config file, hardware) and multiple ways of using them
        // (depending on whether the parameter is writable at runtime).
        //
        // TODO: check mandatory (where?)
        let def_expr = match default {
            None => None,
            Some(def) => Some(try_!(syn::parse_str::<Expr>(&def),
                                    "unparseable default value for param {}", name))
        };
        let def_option = def_expr.map_or(quote!(None::<fn() -> #type_repr>),
                                         |expr| quote!(Some(|| #expr)));
        let upd_closure = if swonly && !readonly {
            quote! { |slf, v| slf.#update_method(v) }
        } else {
            quote! { |_, _| Ok(()) }
        };
        let init_stanza = quote! {
            if let Err(e) = self.init_parameter(#name, |slf| &mut slf.cache.#name_id, &*#type_static,
                                                #upd_closure, #swonly, #readonly, #def_option) {
                return Err(e.amend(concat!("while initializing parameter ", #name)));
            }
        };
        match (swonly, readonly) {
            (true, _) => init_params_swonly.push(init_stanza),
            (false, false) => init_params_write.push(init_stanza),
            (false, true) => init_params_read.push(init_stanza),
        }

        // Generate the parameter's entry in the descriptive data.  If the
        // visibility is "none", the parameter is completely hidden, but can
        // still be manipulated when known to exist.
        if visibility != "none" {
            let unit_entry = if !unit.is_empty() {
                quote! { "unit": #unit, }
            } else { quote! {} };
            descriptive.push(quote! {
                #name: {
                    "description": #doc,
                    "datainfo": serde_json::to_value(&*#type_static).unwrap(),
                    "readonly": #readonly,
                    "group": #group,
                    "visibility": #visibility,
                    #unit_entry
                },
            });
        }
    }

    // Handling for commands is very similar to, but simpler than, parameter handling,
    // since commands do not have to do initialization, caching, or polling.
    for (span,
         SecopCommand { name, doc, argtype, restype, group, visibility }) in commands {
        if !lc_names.insert(name.to_lowercase()) {
            try_!(Err(Error::new(span, "param/cmd name is not unique")));
        }
        if !VISIBILITIES.iter().any(|&v| v == visibility) {
            try_!(Err(Error::new(span, "visibility is not an allowed value")));
        }

        let argtype_static = format_ident!("CMD_ARG_{}", name);
        let (argtype_t, argtype) = try_!(crate::parse_datainfo(span, &argtype));
        let restype_static = format_ident!("CMD_RES_{}", name);
        let (restype_t, restype) = try_!(crate::parse_datainfo(span, &restype));
        let do_method = format_ident!("do_{}", name);
        statics.push(quote! {
            static ref #argtype_static: #argtype_t = #argtype;
            static ref #restype_static: #restype_t = #restype;
        });
        cmd_arms.push(quote! {
            #name => (|| {
                let result_r = self.#do_method(#argtype_static.from_json(&arg)?)?;
                let result = #restype_static.to_json(result_r)?;
                Ok(json!([result, {"t": localtime()}]))
            })()
        });
        if visibility != "none" {
            descriptive.push(quote! {
                #name: {
                    "description": #doc,
                    "datainfo": {"type": "command",
                                 "argument": serde_json::to_value(&*#argtype_static).unwrap(),
                                 "result": serde_json::to_value(&*#restype_static).unwrap()},
                    "group": #group,
                    "visibility": #visibility,
                },
            });
        }
    }

    // So that we can interpolate it twice below.
    let poll_busy_params = &poll_busy_params;

    // Generate the final code.  Most is contained in the impl of ModuleBase,
    // some other bits are done below.
    let generated_impl = input.gen_impl(quote! {
        // Try to `use` all necessary APIs here.
        use serde_json::{Value, json};
        use lazy_static::lazy_static;
        use mlzutil::time::localtime;
        use secop_core::errors::{Error, ErrorKind, Result};
        use secop_core::proto::Msg;
        use secop_core::module::ModuleBase;
        use secop_core::types::TypeInfo;

        lazy_static! {
            #( #statics )*
        }

        gen impl ModuleBase for @Self {
            fn internals(&self) -> &ModInternals { &self.internals }
            fn internals_mut(&mut self) -> &mut ModInternals { &mut self.internals }

            fn describe(&self) -> Value {
                json!({
                    "description": self.config().description,
                    "interface_classes": ["Drivable"], // TODO
                    "features": [],
                    "visibility": self.config().visibility,
                    "group": self.config().group,
                    "accessibles": {
                        #( #descriptive )*
                    }
                })
            }

            fn read(&mut self, param: &str) -> Result<Value> {
                debug!("reading parameter {}", param);
                let result = match param {
                    #( #par_read_arms, )*
                    _ => Err(Error::no_param())
                };
                match result {
                    Ok((value, time)) => Ok(json!([value, {"t": time}])),
                    Err(e) => {
                        error!("while reading parameter {}: {}", param, e);
                        Err(e)
                    }
                }
            }

            fn change(&mut self, param: &str, value: Value) -> Result<Value> {
                debug!("changing parameter {} to {}", param, value);
                let result = match param {
                    #( #par_write_arms, )*
                    _ => Err(Error::no_param())
                };
                if let Err(ref e) = result {
                    error!("while changing parameter {} to {}: {}", param, value, e);
                }
                result
            }

            fn command(&mut self, cmd: &str, arg: Value) -> Result<Value> {
                debug!("executing command {} with arg {}", cmd, arg);
                let result = match cmd {
                    #( #cmd_arms, )*
                    _ => Err(Error::no_command())
                };
                if let Err(ref e) = result {
                    error!("while executing command {} with arg {}: {}", cmd, arg, e);
                }
                result
            }

            fn activate_updates(&mut self) -> Vec<Msg> {
                let mut res = Vec::new();
                #( #activate_updates )*
                res
            }

            fn init_params(&mut self) -> Result<()> {
                // Initials that are written are processed first, so that the initial
                // read for the other parameters makes use of the written ones already.
                #( #init_params_swonly )*
                #( #init_params_write )*
                #( #init_params_read )*
                Ok(())
            }

            fn poll_normal(&mut self, n: usize) {
                // The parameters with special busy-poll handling are not polled here,
                // to avoid polling them twice at the almost same time.
                if self.cache.status.0 != StatusConst::Busy {
                    #( #poll_busy_params )*
                }
                #( #poll_other_params )*
            }

            fn poll_busy(&mut self, n: usize) {
                if self.cache.status.0 == StatusConst::Busy {
                    #( #poll_busy_params )*
                }
            }
        }
    });

    // Implement Drop to be able to call the teardown function in every case,
    // especially on panic.  (There is no inherent advantage to the user directly
    // implementing Drop, but this puts setup and teardown closer together
    // in the Module trait.)
    let drop_impl = input.gen_impl(quote! {
        gen impl Drop for @Self {
            fn drop(&mut self) {
                self.teardown();
            }
        }
    });

    let generated = quote! {
        #[derive(Default)]
        #vis struct #param_cache_name {
            #( #param_cache )*
        }

        #generated_impl
        #drop_impl
    };
    // println!("{}", generated);
    generated
}
