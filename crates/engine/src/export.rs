//! CSV export of crawl data, shared by the CLI and the GUI.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::store::{Store, Table, value_text};

/// The whole-table exports written by `export_all`.
pub const STANDARD_EXPORTS: &[(&str, &str)] = &[
    ("SELECT * FROM urls ORDER BY depth, url", "urls.csv"),
    ("SELECT * FROM links ORDER BY from_url", "links.csv"),
    ("SELECT * FROM meta", "meta.csv"),
];

/// Write an in-memory table as CSV. A UTF-8 BOM is prepended so Excel
/// opens non-ASCII text correctly.
pub fn write_csv(t: &Table, path: &Path) -> anyhow::Result<()> {
    let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
    w.write_all(b"\xEF\xBB\xBF")?;
    let mut writer = csv::Writer::from_writer(&mut w);
    writer.write_record(&t.columns)?;
    for row in &t.rows {
        writer.write_record(row.iter().map(value_text))?;
    }
    writer.flush()?;
    drop(writer);
    w.flush()?;
    Ok(())
}

/// Run `sql` against the store and write the result to `path`.
pub fn export_query(store: &Store, sql: &str, path: &Path) -> anyhow::Result<()> {
    write_csv(&store.table(sql)?, path)
}

/// Write urls.csv, links.csv and meta.csv into `dir`; returns the files.
pub fn export_all(store: &Store, dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    STANDARD_EXPORTS
        .iter()
        .map(|(sql, name)| {
            let path = dir.join(name);
            export_query(store, sql, &path)?;
            Ok(path)
        })
        .collect()
}
