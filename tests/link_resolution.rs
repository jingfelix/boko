//! Test link resolution for different formats.

use boko::Book;
use sha1_smol::Sha1;
use std::io::{Cursor, Write};
use std::path::Path;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::from(bytes).hexdigest()
}

fn epub_with_body_toc_anchor() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    zip.start_file("content.opf", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Body Anchor</dc:title><dc:language>en</dc:language><dc:identifier id="id">body-anchor</dc:identifier></metadata><manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest><spine toc="ncx"><itemref idref="chapter"/></spine></package>"#).unwrap();
    zip.start_file("toc.ncx", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="body-anchor"/></head><docTitle><text>Body Anchor</text></docTitle><navMap><navPoint id="chapter" playOrder="1"><navLabel><text>Chapter</text></navLabel><content src="chapter.xhtml#chapter-start"/></navPoint></navMap></ncx>"#).unwrap();
    zip.start_file("chapter.xhtml", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter</title></head><body id="chapter-start"><h1>Chapter</h1><p>Text</p></body></html>"#).unwrap();

    zip.finish().unwrap().into_inner()
}
#[test]
fn test_azw3_toc_resolution() {
    let path = "tests/fixtures/epictetus.azw3";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping test - fixture not found: {path}");
        return;
    }

    let book = Book::open(path).expect("Should open AZW3");

    // Before resolve_links: TOC hrefs don't have fragments
    // Resolve links (also resolves TOC)
    let _ = book.resolve_links().expect("Should resolve links");

    // After resolve_links: TOC hrefs should have fragments
    let toc = book.toc();

    fn count_with_fragments(entries: &[boko::model::TocEntry]) -> (usize, usize, usize) {
        let mut total = 0;
        let mut with_fragment = 0;
        let mut with_target = 0;
        for entry in entries {
            total += 1;
            if entry.href.contains('#') {
                with_fragment += 1;
            }
            if entry.target.is_some() {
                with_target += 1;
            }
            let (t, f, tgt) = count_with_fragments(&entry.children);
            total += t;
            with_fragment += f;
            with_target += tgt;
        }
        (total, with_fragment, with_target)
    }

    let (_total, with_fragment, with_target) = count_with_fragments(toc);
    // At least some TOC entries should have fragments
    assert!(
        with_fragment > 0,
        "Expected some TOC entries to have fragments, got {with_fragment}"
    );

    // TOC entries should have targets
    assert!(
        with_target > 0,
        "Expected some TOC entries to have targets, got {with_target}"
    );

    // Every TOC entry should have a unique href (catches insert_pos vs start_pos bug)
    assert_unique_toc_hrefs(toc, "AZW3");
}

/// Helper to collect all TOC hrefs recursively.
fn collect_toc_hrefs(entries: &[boko::model::TocEntry], hrefs: &mut Vec<String>) {
    for entry in entries {
        hrefs.push(entry.href.clone());
        collect_toc_hrefs(&entry.children, hrefs);
    }
}

/// Helper to assert all TOC entries have unique hrefs.
fn assert_unique_toc_hrefs(toc: &[boko::model::TocEntry], format_name: &str) {
    use std::collections::HashMap;

    let mut all_hrefs = Vec::new();
    collect_toc_hrefs(toc, &mut all_hrefs);

    let mut href_counts: HashMap<&String, usize> = HashMap::new();
    for href in &all_hrefs {
        *href_counts.entry(href).or_default() += 1;
    }
    let unique_count = href_counts.len();
    assert_eq!(
        all_hrefs.len(),
        unique_count,
        "{format_name}: Every TOC entry should have a unique href"
    );
}

#[test]
fn test_epub_toc_resolution() {
    let path = "tests/fixtures/epictetus.epub";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping test - fixture not found: {path}");
        return;
    }

    let book = Book::open(path).expect("Should open EPUB");
    let _ = book.resolve_links().expect("Should resolve links");

    assert_unique_toc_hrefs(book.toc(), "EPUB");
}

#[test]
fn test_epub_body_id_toc_resolves_to_chapter_start() {
    let epub = epub_with_body_toc_anchor();
    let book = Book::from_bytes(&epub, boko::Format::Epub).expect("open EPUB");

    book.resolve_links().expect("resolve links");

    assert_eq!(
        book.toc()[0].target,
        Some(boko::model::AnchorTarget::Chapter(boko::import::ChapterId(
            0
        )))
    );
}

#[test]
fn test_mobi_toc_resolution() {
    let path = "tests/fixtures/epictetus.mobi";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping test - fixture not found: {path}");
        return;
    }

    let book = Book::open(path).expect("Should open MOBI");
    let _ = book.resolve_links().expect("Should resolve links");

    assert_unique_toc_hrefs(book.toc(), "MOBI");
}

#[test]
fn test_kfx_toc_resolution() {
    let path = "tests/fixtures/epictetus.kfx";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping test - fixture not found: {path}");
        return;
    }

    let book = Book::open(path).expect("Should open KFX");
    let _ = book.resolve_links().expect("Should resolve links");

    assert_unique_toc_hrefs(book.toc(), "KFX");
}

#[test]
fn test_kfx_named_resource_returns_binary_asset() {
    let path = "tests/fixtures/epictetus.kfx";
    if !Path::new(path).exists() {
        eprintln!("Skipping test - fixture not found: {path}");
        return;
    }

    let book = Book::open(path).expect("Should open KFX");

    let expected = [
        ("resource/rsrc7", "dae4335aa095d1109e81a413cea05e1a3225e4ed"),
        (
            "resource/rsrc1DT",
            "5d4c6c7573d11baff232c9ee8381f2012a4c9be2",
        ),
        (
            "resource/rsrc1DU",
            "4a5dc446b5a102bbb11e9439ee586bcc5a4e0811",
        ),
    ];

    for (resource_name, expected_sha1) in expected {
        let bytes = book
            .load_asset(resource_name)
            .expect("Expected named resource to exist in epictetus.kfx");

        assert!(
            !bytes.starts_with(&[0xE0, 0x01, 0x00, 0xEA]),
            "Expected binary media bytes for {}, got Ion metadata payload ({} bytes)",
            resource_name,
            bytes.len()
        );
        assert!(
            bytes.len() > 256,
            "Expected substantial media payload for {}, got {} bytes",
            resource_name,
            bytes.len()
        );
        assert_eq!(
            sha1_hex(bytes.as_slice()),
            expected_sha1,
            "Unexpected SHA-1 for {resource_name} in epictetus fixture"
        );
    }
}

#[test]
fn test_kfx_direct_asset_id_1102_matches_named_resource_and_hash() {
    let path = "tests/fixtures/epictetus.kfx";
    if !Path::new(path).exists() {
        eprintln!("Skipping test - fixture not found: {path}");
        return;
    }

    let book = Book::open(path).expect("Should open KFX");

    assert!(
        book.list_assets().iter().any(|p| p == "#1102"),
        "Expected #1102 to be listed as a KFX asset"
    );

    let id_bytes = book
        .load_asset("#1102")
        .expect("Should load #1102 asset bytes");
    let id_sha1 = sha1_hex(id_bytes.as_slice());

    assert_eq!(
        id_sha1, "5d4c6c7573d11baff232c9ee8381f2012a4c9be2",
        "Unexpected SHA-1 for fixture asset #1102"
    );

    let named_sha1 = book
        .load_asset("resource/rsrc1DT")
        .map(|bytes| sha1_hex(bytes.as_slice()))
        .expect("Should load resource/rsrc1DT by name");
    assert_eq!(
        named_sha1, id_sha1,
        "Expected resource/rsrc1DT to match #1102 payload hash"
    );
}
