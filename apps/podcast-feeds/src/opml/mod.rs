//! OPML 2.0 subscription list import and export. Ports
//! `OPMLImport.swift` and `OPMLExport.swift`.

pub mod export;
pub mod import;

pub use export::{export_opml, export_opml_with};
pub use import::{import_opml, OpmlError};
