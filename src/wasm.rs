//! WASM bindings for browser-based ebook conversion.
//!
//! This module exposes the core conversion functions to JavaScript via wasm-bindgen.

use std::io::Cursor;
use wasm_bindgen::prelude::*;

use crate::Book;
use crate::model::{CollectionInfo, Contributor, Format, Metadata, TocEntry};

#[derive(Debug)]
enum OptionalPatch<T> {
    Missing,
    Set(T),
    Clear,
}

impl<T> Default for OptionalPatch<T> {
    fn default() -> Self {
        Self::Missing
    }
}

impl<'de, T> serde::Deserialize<'de> for OptionalPatch<T>
where
    T: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(|value| match value {
            Some(value) => Self::Set(value),
            None => Self::Clear,
        })
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct MetadataPatch {
    title: Option<String>,
    authors: Option<Vec<String>>,
    language: Option<String>,
    identifier: Option<String>,
    publisher: OptionalPatch<String>,
    description: OptionalPatch<String>,
    subjects: Option<Vec<String>>,
    date: OptionalPatch<String>,
    rights: OptionalPatch<String>,
    #[serde(alias = "coverImage")]
    cover_image: OptionalPatch<String>,
    #[serde(alias = "modifiedDate")]
    modified_date: OptionalPatch<String>,
    contributors: Option<Vec<ContributorPatch>>,
    #[serde(alias = "titleSort")]
    title_sort: OptionalPatch<String>,
    #[serde(alias = "authorSort")]
    author_sort: OptionalPatch<String>,
    collection: OptionalPatch<CollectionPatch>,
}

#[derive(Debug, serde::Deserialize)]
struct ContributorPatch {
    name: String,
    #[serde(alias = "fileAs")]
    file_as: Option<String>,
    role: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct CollectionPatch {
    name: String,
    #[serde(alias = "collectionType")]
    collection_type: Option<String>,
    position: Option<f64>,
}

#[derive(Debug, serde::Serialize)]
struct MetadataView {
    title: String,
    authors: Vec<String>,
    language: String,
    identifier: String,
    publisher: Option<String>,
    description: Option<String>,
    subjects: Vec<String>,
    date: Option<String>,
    rights: Option<String>,
    cover_image: Option<String>,
    modified_date: Option<String>,
    contributors: Vec<ContributorView>,
    title_sort: Option<String>,
    author_sort: Option<String>,
    collection: Option<CollectionView>,
}

#[derive(Debug, serde::Serialize)]
struct ContributorView {
    name: String,
    file_as: Option<String>,
    role: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct CollectionView {
    name: String,
    collection_type: Option<String>,
    position: Option<f64>,
}

impl From<&Metadata> for MetadataView {
    fn from(metadata: &Metadata) -> Self {
        Self {
            title: metadata.title.clone(),
            authors: metadata.authors.clone(),
            language: metadata.language.clone(),
            identifier: metadata.identifier.clone(),
            publisher: metadata.publisher.clone(),
            description: metadata.description.clone(),
            subjects: metadata.subjects.clone(),
            date: metadata.date.clone(),
            rights: metadata.rights.clone(),
            cover_image: metadata.cover_image.clone(),
            modified_date: metadata.modified_date.clone(),
            contributors: metadata
                .contributors
                .iter()
                .map(|contributor| ContributorView {
                    name: contributor.name.clone(),
                    file_as: contributor.file_as.clone(),
                    role: contributor.role.clone(),
                })
                .collect(),
            title_sort: metadata.title_sort.clone(),
            author_sort: metadata.author_sort.clone(),
            collection: metadata
                .collection
                .as_ref()
                .map(|collection| CollectionView {
                    name: collection.name.clone(),
                    collection_type: collection.collection_type.clone(),
                    position: collection.position,
                }),
        }
    }
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn apply_optional_patch<T>(target: &mut Option<T>, patch: OptionalPatch<T>) {
    match patch {
        OptionalPatch::Missing => {}
        OptionalPatch::Set(value) => *target = Some(value),
        OptionalPatch::Clear => *target = None,
    }
}

fn apply_metadata_patch(book: &mut Book, metadata_json: &str) -> Result<(), JsValue> {
    if metadata_json.trim().is_empty() {
        return Ok(());
    }

    let patch: MetadataPatch = serde_json::from_str(metadata_json)
        .map_err(|e| JsValue::from_str(&format!("invalid metadata JSON: {e}")))?;
    let metadata = book.metadata_mut();

    if let Some(title) = patch.title {
        metadata.title = title;
    }
    if let Some(authors) = patch.authors {
        metadata.authors = authors;
    }
    if let Some(language) = patch.language {
        metadata.language = language;
    }
    if let Some(identifier) = patch.identifier {
        metadata.identifier = identifier;
    }
    apply_optional_patch(&mut metadata.publisher, patch.publisher);
    apply_optional_patch(&mut metadata.description, patch.description);
    if let Some(subjects) = patch.subjects {
        metadata.subjects = subjects;
    }
    apply_optional_patch(&mut metadata.date, patch.date);
    apply_optional_patch(&mut metadata.rights, patch.rights);
    apply_optional_patch(&mut metadata.cover_image, patch.cover_image);
    apply_optional_patch(&mut metadata.modified_date, patch.modified_date);
    if let Some(contributors) = patch.contributors {
        metadata.contributors = contributors
            .into_iter()
            .map(|contributor| Contributor {
                name: contributor.name,
                file_as: contributor.file_as,
                role: contributor.role,
            })
            .collect();
    }
    apply_optional_patch(&mut metadata.title_sort, patch.title_sort);
    apply_optional_patch(&mut metadata.author_sort, patch.author_sort);
    match patch.collection {
        OptionalPatch::Missing => {}
        OptionalPatch::Set(collection) => {
            metadata.collection = Some(CollectionInfo {
                name: collection.name,
                collection_type: collection.collection_type,
                position: collection.position,
            });
        }
        OptionalPatch::Clear => metadata.collection = None,
    }

    Ok(())
}

fn convert_formats(data: &[u8], input: Format, output: Format) -> Result<Vec<u8>, JsValue> {
    let mut book = Book::from_bytes(data, input).map_err(js_error)?;
    export_book(&mut book, output)
}

fn convert_formats_with_options(
    data: &[u8],
    input: Format,
    output: Format,
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    let mut book = Book::from_bytes(data, input).map_err(js_error)?;
    apply_metadata_patch(&mut book, metadata_json)?;
    if !cover_image.is_empty() {
        book.set_cover(cover_image.to_vec()).map_err(js_error)?;
    }
    export_book(&mut book, output)
}

fn export_book(book: &mut Book, format: Format) -> Result<Vec<u8>, JsValue> {
    let mut output = Cursor::new(Vec::new());
    book.export(format, &mut output).map_err(js_error)?;
    Ok(output.into_inner())
}

/// Initialize panic hook for better error messages in the browser console.
#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "wasm")]
    console_error_panic_hook::set_once();
}

/// Parse a format name (as used by the JS API) into a [`Format`].
fn parse_format(name: &str) -> Result<Format, JsValue> {
    match name.to_ascii_lowercase().as_str() {
        "epub" => Ok(Format::Epub),
        "azw3" => Ok(Format::Azw3),
        "mobi" | "azw" => Ok(Format::Mobi),
        "kfx" => Ok(Format::Kfx),
        "markdown" | "md" => Ok(Format::Markdown),
        _ => Err(JsValue::from_str(&format!("unknown format: {name}"))),
    }
}

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Read book metadata as a JSON object.
///
/// `format` accepts "epub", "azw3", "mobi", "azw", or "kfx".
#[wasm_bindgen]
pub fn read_metadata_json(data: &[u8], format: &str) -> Result<String, JsValue> {
    let book = Book::from_bytes(data, parse_format(format)?).map_err(js_error)?;
    let view = MetadataView::from(book.metadata());
    serde_json::to_string(&view).map_err(js_error)
}

/// Convert an ebook from one format to another.
///
/// `from` and `to` are format names: `"epub"`, `"azw3"`, `"mobi"`, `"kfx"`,
/// or `"markdown"` (`"md"`). Any importable `from` (EPUB, AZW3, MOBI, KFX)
/// can be converted to any exportable `to` (EPUB, AZW3, KFX, Markdown).
///
/// Takes the raw input bytes and returns the converted output bytes
/// (UTF-8 text for Markdown).
#[wasm_bindgen]
pub fn convert(data: &[u8], from: &str, to: &str) -> Result<Vec<u8>, JsValue> {
    let from = parse_format(from)?;
    let to = parse_format(to)?;

    if !from.can_import() {
        return Err(JsValue::from_str(&format!(
            "format not supported as input: {from:?}"
        )));
    }
    if !to.can_export() {
        return Err(JsValue::from_str(&format!(
            "format not supported as output: {to:?}"
        )));
    }

    let book = Book::from_bytes(data, from).map_err(js_err)?;

    let mut output = Cursor::new(Vec::new());
    book.export(to, &mut output).map_err(js_err)?;

    Ok(output.into_inner())
}

/// Convert EPUB to AZW3.
#[wasm_bindgen]
pub fn epub_to_azw3(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Epub, Format::Azw3)
}

/// Convert EPUB to AZW3 after applying metadata and/or cover edits.
///
/// `metadata_json` is a JSON object with Metadata field names. Empty string means
/// no metadata changes. `cover_image` may be an empty byte array to keep the
/// existing cover.
#[wasm_bindgen]
pub fn epub_to_azw3_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(data, Format::Epub, Format::Azw3, metadata_json, cover_image)
}

/// Inspect an ebook's metadata without converting it.
///
/// `from` is the input format name (see [`convert`]). Returns a JSON string:
/// `{"title": ..., "authors": [...], "language": ..., "chapters": n, "toc_entries": n}`.
/// Call `JSON.parse` on the result in JavaScript.
#[wasm_bindgen]
pub fn book_info(data: &[u8], from: &str) -> Result<JsValue, JsValue> {
    let from = parse_format(from)?;
    if !from.can_import() {
        return Err(JsValue::from_str(&format!(
            "format not supported as input: {from:?}"
        )));
    }

    let book = Book::from_bytes(data, from).map_err(js_err)?;
    let meta = book.metadata();

    fn count_toc(entries: &[TocEntry]) -> usize {
        entries.iter().map(|e| 1 + count_toc(&e.children)).sum()
    }

    fn json_string(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    let authors: Vec<String> = meta.authors.iter().map(|a| json_string(a)).collect();
    let json = format!(
        "{{\"title\":{},\"authors\":[{}],\"language\":{},\"chapters\":{},\"toc_entries\":{}}}",
        json_string(&meta.title),
        authors.join(","),
        json_string(&meta.language),
        book.spine().len(),
        count_toc(book.toc()),
    );

    Ok(JsValue::from_str(&json))
}

/// Convert EPUB to KFX.
#[wasm_bindgen]
pub fn epub_to_kfx(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Epub, Format::Kfx)
}

/// Convert EPUB to KFX after applying metadata and/or cover edits.
#[wasm_bindgen]
pub fn epub_to_kfx_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(data, Format::Epub, Format::Kfx, metadata_json, cover_image)
}

/// Convert AZW3 to EPUB.
///
/// Takes raw AZW3 bytes and returns EPUB bytes.
#[wasm_bindgen]
pub fn azw3_to_epub(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Azw3, Format::Epub)
}

/// Convert AZW3 to EPUB after applying metadata and/or cover edits.
#[wasm_bindgen]
pub fn azw3_to_epub_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(data, Format::Azw3, Format::Epub, metadata_json, cover_image)
}

/// Convert KFX to EPUB.
///
/// Takes raw KFX bytes and returns EPUB bytes.
#[wasm_bindgen]
pub fn kfx_to_epub(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Kfx, Format::Epub)
}

/// Convert KFX to EPUB after applying metadata and/or cover edits.
#[wasm_bindgen]
pub fn kfx_to_epub_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(data, Format::Kfx, Format::Epub, metadata_json, cover_image)
}

/// Convert MOBI to EPUB.
///
/// Takes raw MOBI bytes and returns EPUB bytes.
/// Handles both legacy MOBI and modern AZW3 (KF8) formats.
#[wasm_bindgen]
pub fn mobi_to_epub(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Mobi, Format::Epub)
}

/// Convert MOBI to EPUB after applying metadata and/or cover edits.
#[wasm_bindgen]
pub fn mobi_to_epub_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(data, Format::Mobi, Format::Epub, metadata_json, cover_image)
}

/// Convert MOBI to AZW3 (upgrade legacy MOBI to KF8 format).
///
/// Takes raw MOBI bytes and returns AZW3 bytes.
#[wasm_bindgen]
pub fn mobi_to_azw3(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Mobi, Format::Azw3)
}

/// Convert MOBI to AZW3 after applying metadata and/or cover edits.
#[wasm_bindgen]
pub fn mobi_to_azw3_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(data, Format::Mobi, Format::Azw3, metadata_json, cover_image)
}

/// Convert EPUB to Markdown.
///
/// Takes raw EPUB bytes and returns Markdown text as UTF-8 bytes.
#[wasm_bindgen]
pub fn epub_to_markdown(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Epub, Format::Markdown)
}

/// Convert EPUB to Markdown after applying metadata and/or cover edits.
#[wasm_bindgen]
pub fn epub_to_markdown_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(
        data,
        Format::Epub,
        Format::Markdown,
        metadata_json,
        cover_image,
    )
}

/// Convert AZW3 to Markdown.
///
/// Takes raw AZW3 bytes and returns Markdown text as UTF-8 bytes.
#[wasm_bindgen]
pub fn azw3_to_markdown(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Azw3, Format::Markdown)
}

/// Convert AZW3 to Markdown after applying metadata and/or cover edits.
#[wasm_bindgen]
pub fn azw3_to_markdown_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(
        data,
        Format::Azw3,
        Format::Markdown,
        metadata_json,
        cover_image,
    )
}

/// Convert KFX to Markdown.
///
/// Takes raw KFX bytes and returns Markdown text as UTF-8 bytes.
#[wasm_bindgen]
pub fn kfx_to_markdown(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Kfx, Format::Markdown)
}

/// Convert KFX to Markdown after applying metadata and/or cover edits.
#[wasm_bindgen]
pub fn kfx_to_markdown_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(
        data,
        Format::Kfx,
        Format::Markdown,
        metadata_json,
        cover_image,
    )
}

/// Convert MOBI to Markdown.
///
/// Takes raw MOBI bytes and returns Markdown text as UTF-8 bytes.
#[wasm_bindgen]
pub fn mobi_to_markdown(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    convert_formats(data, Format::Mobi, Format::Markdown)
}

/// Convert MOBI to Markdown after applying metadata and/or cover edits.
#[wasm_bindgen]
pub fn mobi_to_markdown_with_options(
    data: &[u8],
    metadata_json: &str,
    cover_image: &[u8],
) -> Result<Vec<u8>, JsValue> {
    convert_formats_with_options(
        data,
        Format::Mobi,
        Format::Markdown,
        metadata_json,
        cover_image,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_book() -> Book {
        let epub = include_bytes!("../tests/fixtures/epictetus.epub");
        Book::from_bytes(epub, Format::Epub).unwrap()
    }

    #[test]
    fn apply_metadata_patch_updates_fields() {
        let mut book = empty_book();
        apply_metadata_patch(
            &mut book,
            r#"{
                "title": "New Title",
                "authors": ["One", "Two"],
                "publisher": "Publisher",
                "subjects": ["A", "B"],
                "titleSort": "Title, New",
                "contributors": [{"name": "Translator", "role": "trl", "fileAs": "Translator"}],
                "collection": {"name": "Series", "collectionType": "series", "position": 2}
            }"#,
        )
        .unwrap();

        let metadata = book.metadata();
        assert_eq!(metadata.title, "New Title");
        assert_eq!(metadata.authors, vec!["One", "Two"]);
        assert_eq!(metadata.publisher.as_deref(), Some("Publisher"));
        assert_eq!(metadata.subjects, vec!["A", "B"]);
        assert_eq!(metadata.title_sort.as_deref(), Some("Title, New"));
        assert_eq!(metadata.contributors[0].role.as_deref(), Some("trl"));
        assert_eq!(metadata.collection.as_ref().unwrap().position, Some(2.0));
    }

    #[test]
    fn apply_metadata_patch_can_clear_optional_fields() {
        let mut book = empty_book();
        book.metadata_mut().publisher = Some("Publisher".to_string());
        book.metadata_mut().collection = Some(CollectionInfo {
            name: "Series".to_string(),
            collection_type: Some("series".to_string()),
            position: Some(1.0),
        });

        apply_metadata_patch(&mut book, r#"{"publisher": null, "collection": null}"#).unwrap();

        assert!(book.metadata().publisher.is_none());
        assert!(book.metadata().collection.is_none());
    }

    #[test]
    fn read_metadata_json_returns_book_metadata() {
        let epub = include_bytes!("../tests/fixtures/epictetus.epub");
        let metadata = read_metadata_json(epub, "epub").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&metadata).unwrap();

        assert!(
            parsed["title"]
                .as_str()
                .is_some_and(|title| !title.is_empty())
        );
        assert!(parsed["authors"].as_array().is_some());
    }
}
