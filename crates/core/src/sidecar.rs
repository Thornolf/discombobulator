use std::io::Write;
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::Writer;
use quick_xml::events::{BytesStart, Event};
use thiserror::Error;

use crate::model::{Decision, Mark, Photo};

const XMP_NS: &str = "http://ns.adobe.com/xap/1.0/";
const XMP_DM_NS: &str = "http://ns.adobe.com/xmp/1.0/DynamicMedia/";
const MANAGED: [&str; 4] = ["xmp:Rating", "xmp:Label", "xmpDM:pick", "xmpDM:good"];
pub const MAX_SIDECAR_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("existing sidecar is not valid XMP: {0}")]
    Invalid(String),
    #[error("existing sidecar has no rdf:Description")]
    NoDescription,
    #[error("existing sidecar declares a DOCTYPE")]
    Doctype,
    #[error("sidecar is a link or not a regular file")]
    NotRegularFile,
    #[error("existing sidecar is too large")]
    TooLarge,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn sidecar_path(photo: &Photo) -> Option<PathBuf> {
    photo
        .raf
        .as_ref()
        .or(photo.jpg.as_ref())
        .map(|path| path.with_extension("xmp"))
}

pub fn managed_values(decision: &Decision) -> Vec<(&'static str, String)> {
    let mut values = Vec::new();
    if decision.rating > 0 {
        values.push(("xmp:Rating", decision.rating.to_string()));
    }
    let label = match (decision.mark, decision.favorite) {
        (Some(Mark::Unsure), _) => Some("Yellow"),
        (_, true) => Some("Purple"),
        _ => None,
    };
    if let Some(label) = label {
        values.push(("xmp:Label", label.to_owned()));
    }
    match decision.mark {
        Some(Mark::Approved) => {
            values.push(("xmpDM:pick", "1".to_owned()));
            values.push(("xmpDM:good", "true".to_owned()));
        }
        Some(Mark::Delete) => {
            values.push(("xmpDM:pick", "-1".to_owned()));
            values.push(("xmpDM:good", "false".to_owned()));
        }
        Some(Mark::Unsure) | None => {}
    }
    values
}

fn fresh(decision: &Decision) -> String {
    let attributes: String = managed_values(decision)
        .iter()
        .map(|(name, value)| format!("\n    {name}=\"{value}\""))
        .collect();
    format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Discombobulator\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  <rdf:Description rdf:about=\"\"\n    xmlns:xmp=\"{XMP_NS}\"\n    xmlns:xmpDM=\"{XMP_DM_NS}\"{attributes}/>\n </rdf:RDF>\n</x:xmpmeta>\n"
    )
}

fn is_managed(name: &str) -> bool {
    MANAGED.contains(&name)
}

fn rewrite_description(
    start: &BytesStart<'_>,
    decision: &Decision,
) -> Result<BytesStart<'static>, SidecarError> {
    let name = start.name().as_ref().to_owned();
    let mut rewritten = BytesStart::new(name);
    let mut has_xmp = false;
    let mut has_dm = false;
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|error| SidecarError::Invalid(error.to_string()))?;
        let key = attribute.key.as_ref();
        if is_managed(key) {
            continue;
        }
        has_xmp |= key == "xmlns:xmp";
        has_dm |= key == "xmlns:xmpDM";
        rewritten.push_attribute(attribute);
    }
    if !has_xmp {
        rewritten.push_attribute(("xmlns:xmp", XMP_NS));
    }
    if !has_dm {
        rewritten.push_attribute(("xmlns:xmpDM", XMP_DM_NS));
    }
    for (name, value) in managed_values(decision) {
        rewritten.push_attribute((name, value.as_str()));
    }
    Ok(rewritten)
}

pub fn merge(existing: &str, decision: &Decision) -> Result<String, SidecarError> {
    let mut reader = Reader::from_str(existing);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::new());
    let mut rewritten = false;
    let mut skip_depth = 0usize;
    let mut depth = 0usize;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| SidecarError::Invalid(error.to_string()))?;
        if skip_depth > 0 {
            match event {
                Event::Start(_) => skip_depth += 1,
                Event::End(_) => skip_depth -= 1,
                Event::Eof => return Err(SidecarError::Invalid("unclosed element".into())),
                _ => {}
            }
            continue;
        }
        let output = match event {
            Event::Eof if depth == 0 => break,
            Event::Eof => return Err(SidecarError::Invalid("unclosed element".into())),
            Event::DocType(_) => return Err(SidecarError::Doctype),
            Event::Start(start) if is_managed(start.name().as_ref()) => {
                skip_depth = 1;
                continue;
            }
            Event::Empty(start) if is_managed(start.name().as_ref()) => continue,
            Event::Start(start) if !rewritten && start.name().as_ref() == "rdf:Description" => {
                rewritten = true;
                Event::Start(rewrite_description(&start, decision)?)
            }
            Event::Empty(start) if !rewritten && start.name().as_ref() == "rdf:Description" => {
                rewritten = true;
                Event::Empty(rewrite_description(&start, decision)?)
            }
            other => other,
        };
        match output {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        writer
            .write_event(output)
            .map_err(|error| SidecarError::Invalid(error.to_string()))?;
    }
    if !rewritten {
        return Err(SidecarError::NoDescription);
    }
    String::from_utf8(writer.into_inner()).map_err(|error| SidecarError::Invalid(error.to_string()))
}

fn read_existing(path: &Path) -> Result<Option<String>, SidecarError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() {
        return Err(SidecarError::NotRegularFile);
    }
    if metadata.len() > MAX_SIDECAR_BYTES {
        return Err(SidecarError::TooLarge);
    }
    let bytes = std::fs::read(path)?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| SidecarError::Invalid(error.to_string()))
}

pub fn write(path: &Path, decision: &Decision) -> Result<(), SidecarError> {
    let content = match read_existing(path)? {
        Some(existing) => merge(&existing, decision)?,
        None => fresh(decision),
    };
    let parent = path
        .parent()
        .ok_or_else(|| SidecarError::Io(std::io::Error::other("sidecar has no folder")))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".discombobulator-")
        .suffix(".xmp.tmp")
        .tempfile_in(parent)?;
    temporary.write_all(content.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(mark: Option<Mark>, favorite: bool, rating: u8) -> Decision {
        Decision {
            mark,
            favorite,
            rating,
        }
    }

    #[test]
    fn maps_marks_to_lightroom_fields() {
        assert_eq!(
            managed_values(&decision(Some(Mark::Approved), false, 3)),
            vec![
                ("xmp:Rating", "3".to_owned()),
                ("xmpDM:pick", "1".to_owned()),
                ("xmpDM:good", "true".to_owned())
            ]
        );
        assert_eq!(
            managed_values(&decision(Some(Mark::Delete), false, 0)),
            vec![
                ("xmpDM:pick", "-1".to_owned()),
                ("xmpDM:good", "false".to_owned())
            ]
        );
        assert_eq!(
            managed_values(&decision(Some(Mark::Unsure), true, 0)),
            vec![("xmp:Label", "Yellow".to_owned())]
        );
        assert_eq!(
            managed_values(&decision(None, true, 0)),
            vec![("xmp:Label", "Purple".to_owned())]
        );
    }

    #[test]
    fn fresh_sidecar_is_valid_and_mergeable() -> Result<(), SidecarError> {
        let first = fresh(&decision(Some(Mark::Approved), false, 2));
        let merged = merge(&first, &decision(Some(Mark::Delete), false, 0))?;
        assert!(merged.contains("xmpDM:pick=\"-1\""));
        assert!(!merged.contains("xmpDM:pick=\"1\""));
        assert!(!merged.contains("xmp:Rating"));
        Ok(())
    }

    #[test]
    fn merge_keeps_foreign_content_and_replaces_managed_fields() -> Result<(), SidecarError> {
        let existing = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmp:Rating="5" crs:Exposure2012="+0.35" xmp:Label="Red">
   <xmpDM:pick>1</xmpDM:pick>
   <dc:subject><rdf:Bag><rdf:li>beach &amp; sea</rdf:li></rdf:Bag></dc:subject>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;
        let merged = merge(existing, &decision(Some(Mark::Unsure), false, 1))?;
        assert!(merged.contains(r#"crs:Exposure2012="+0.35""#));
        assert!(merged.contains("beach &amp; sea"));
        assert!(merged.contains("<?xpacket end=\"w\"?>"));
        assert!(merged.contains(r#"xmp:Rating="1""#));
        assert!(merged.contains(r#"xmp:Label="Yellow""#));
        assert!(!merged.contains("Red"));
        assert!(!merged.contains("<xmpDM:pick>"));
        assert!(merged.contains(r#"xmlns:xmpDM="http://ns.adobe.com/xmp/1.0/DynamicMedia/""#));
        Ok(())
    }

    #[test]
    fn refuses_doctype_and_garbage() {
        assert!(matches!(
            merge(
                r#"<!DOCTYPE x [<!ENTITY a "b">]><rdf:Description/>"#,
                &Decision::default()
            ),
            Err(SidecarError::Doctype)
        ));
        assert!(merge("<x:xmpmeta><rdf:Description>", &Decision::default()).is_err());
        assert!(matches!(
            merge("<x:xmpmeta/>", &Decision::default()),
            Err(SidecarError::NoDescription)
        ));
    }

    #[test]
    fn writes_new_sidecar_and_updates_it() -> Result<(), SidecarError> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("DSCF0001.xmp");
        write(&path, &decision(Some(Mark::Approved), true, 4))?;
        write(&path, &decision(None, false, 0))?;
        let content = std::fs::read_to_string(&path)?;
        assert!(!content.contains("xmpDM:pick"));
        assert!(!content.contains("xmp:Label"));
        assert!(content.contains("rdf:Description"));
        let leftovers = std::fs::read_dir(dir.path())?.count();
        assert_eq!(leftovers, 1);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_write_through_a_symlink() -> Result<(), SidecarError> {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("elsewhere.txt");
        std::fs::write(&target, "keep")?;
        let link = dir.path().join("DSCF0001.xmp");
        std::os::unix::fs::symlink(&target, &link)?;
        assert!(matches!(
            write(&link, &Decision::default()),
            Err(SidecarError::NotRegularFile)
        ));
        assert_eq!(std::fs::read_to_string(target)?, "keep");
        Ok(())
    }
}
