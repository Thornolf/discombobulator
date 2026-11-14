use std::borrow::Cow;
use std::io::Write;
use std::path::{Path, PathBuf};

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{Namespace, NamespaceResolver, QName, ResolveResult};
use quick_xml::{NsReader, Writer, XmlVersion};
use thiserror::Error;

use crate::fsx;
use crate::model::{Decision, Mark, Photo};

const XMP_NS: &str = "http://ns.adobe.com/xap/1.0/";
const XMP_DM_NS: &str = "http://ns.adobe.com/xmp/1.0/DynamicMedia/";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub const MAX_SIDECAR_BYTES: u64 = 8 * 1024 * 1024;

struct Field {
    name: &'static str,
    namespace: &'static str,
    local: &'static str,
}

const FIELDS: [Field; 4] = [
    Field {
        name: "xmp:Rating",
        namespace: XMP_NS,
        local: "Rating",
    },
    Field {
        name: "xmp:Label",
        namespace: XMP_NS,
        local: "Label",
    },
    Field {
        name: "xmpDM:pick",
        namespace: XMP_DM_NS,
        local: "pick",
    },
    Field {
        name: "xmpDM:good",
        namespace: XMP_DM_NS,
        local: "good",
    },
];

const PREFIXES: [(&str, &str); 2] = [("xmp", XMP_NS), ("xmpDM", XMP_DM_NS)];

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

fn invalid(error: impl std::fmt::Display) -> SidecarError {
    SidecarError::Invalid(error.to_string())
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
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"Discombobulator\">\n <rdf:RDF xmlns:rdf=\"{RDF_NS}\">\n  <rdf:Description rdf:about=\"\"\n    xmlns:xmp=\"{XMP_NS}\"\n    xmlns:xmpDM=\"{XMP_DM_NS}\"{attributes}/>\n </rdf:RDF>\n</x:xmpmeta>\n"
    )
}

fn namespace_of<'a>(result: &'a ResolveResult<'_>) -> Option<&'a str> {
    match result {
        ResolveResult::Bound(Namespace(namespace)) => Some(namespace),
        ResolveResult::Unknown(prefix) => match prefix.as_str() {
            "xmp" | "xap" => Some(XMP_NS),
            "xmpDM" => Some(XMP_DM_NS),
            _ => None,
        },
        ResolveResult::Unbound => None,
    }
}

fn field_named(
    resolved: (ResolveResult<'_>, quick_xml::name::LocalName<'_>),
) -> Option<&'static str> {
    let (result, local) = resolved;
    let namespace = namespace_of(&result)?;
    FIELDS
        .iter()
        .find(|field| field.namespace == namespace && field.local == local.as_ref())
        .map(|field| field.name)
}

fn is_removable(field: Option<&'static str>, removable: &[&str]) -> bool {
    field.is_some_and(|name| removable.contains(&name))
}

fn escape_attribute(value: &str) -> String {
    quick_xml::escape::escape(value)
        .replace('\n', "&#10;")
        .replace('\t', "&#9;")
}

fn binding_for(
    resolver: &NamespaceResolver,
    own_keys: &[String],
    preferred: &str,
    namespace: &str,
) -> (String, bool) {
    let mut suffix = 0usize;
    loop {
        let prefix = if suffix == 0 {
            preferred.to_owned()
        } else {
            format!("{preferred}{suffix}")
        };
        suffix += 1;
        let declaration = format!("xmlns:{prefix}");
        let probe = format!("{prefix}:x");
        match resolver.resolve_attribute(QName(&probe)).0 {
            ResolveResult::Bound(Namespace(bound)) if bound == namespace => {
                return (prefix, false);
            }
            ResolveResult::Unknown(_) if !own_keys.contains(&declaration) => {
                return (prefix, true);
            }
            _ => {}
        }
    }
}

fn rewrite_description(
    start: &BytesStart<'_>,
    resolver: &NamespaceResolver,
    removable: &[&str],
    values: Option<&[(&'static str, String)]>,
) -> Result<BytesStart<'static>, SidecarError> {
    let mut rewritten = BytesStart::new(start.name().as_ref().to_owned());
    let mut own_keys = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(invalid)?;
        let key = attribute.key.as_ref();
        own_keys.push(key.to_owned());
        if is_removable(
            field_named(resolver.resolve_attribute(attribute.key)),
            removable,
        ) {
            continue;
        }
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(invalid)?;
        rewritten.push_attribute(Attribute {
            key: QName(key),
            value: Cow::Owned(escape_attribute(&value)),
        });
    }
    let Some(values) = values else {
        return Ok(rewritten);
    };
    let mut prefixes = Vec::new();
    for (preferred, namespace) in PREFIXES {
        let (prefix, declare) = binding_for(resolver, &own_keys, preferred, namespace);
        if declare {
            rewritten.push_attribute((format!("xmlns:{prefix}").as_str(), namespace));
        }
        prefixes.push((namespace, prefix));
    }
    for (name, value) in values {
        let Some(field) = FIELDS.iter().find(|field| field.name == *name) else {
            continue;
        };
        let Some((_, prefix)) = prefixes
            .iter()
            .find(|(namespace, _)| *namespace == field.namespace)
        else {
            continue;
        };
        rewritten.push_attribute((format!("{prefix}:{}", field.local).as_str(), value.as_str()));
    }
    Ok(rewritten)
}

fn is_description(resolver: &NamespaceResolver, start: &BytesStart<'_>) -> bool {
    let (result, local) = resolver.resolve_element(start.name());
    matches!(result, ResolveResult::Bound(Namespace(namespace)) if namespace == RDF_NS)
        && local.as_ref() == "Description"
}

pub fn merge(
    existing: &str,
    decision: &Decision,
    previous: &[String],
) -> Result<String, SidecarError> {
    let values = managed_values(decision);
    let removable: Vec<&str> = FIELDS
        .iter()
        .map(|field| field.name)
        .filter(|name| {
            values.iter().any(|(value_name, _)| value_name == name)
                || previous.iter().any(|written| written == name)
        })
        .collect();
    let mut reader = NsReader::from_str(existing);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::new());
    let mut rewritten = false;
    let mut skip_depth = 0usize;
    let mut depth = 0usize;
    loop {
        let event = reader.read_event().map_err(invalid)?;
        let resolver = reader.resolver();
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
            Event::Start(start)
                if is_removable(
                    field_named(resolver.resolve_element(start.name())),
                    &removable,
                ) =>
            {
                skip_depth = 1;
                continue;
            }
            Event::Empty(start)
                if is_removable(
                    field_named(resolver.resolve_element(start.name())),
                    &removable,
                ) =>
            {
                continue;
            }
            Event::Start(start) if is_description(resolver, &start) => {
                let additions = (!rewritten).then_some(values.as_slice());
                rewritten = true;
                Event::Start(rewrite_description(
                    &start, resolver, &removable, additions,
                )?)
            }
            Event::Empty(start) if is_description(resolver, &start) => {
                let additions = (!rewritten).then_some(values.as_slice());
                rewritten = true;
                Event::Empty(rewrite_description(
                    &start, resolver, &removable, additions,
                )?)
            }
            other => other,
        };
        match output {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        writer.write_event(output).map_err(invalid)?;
    }
    if !rewritten {
        return Err(SidecarError::NoDescription);
    }
    String::from_utf8(writer.into_inner()).map_err(invalid)
}

fn validate(content: &str) -> Result<(), SidecarError> {
    let mut reader = NsReader::from_str(content);
    let mut depth = 0usize;
    loop {
        match reader.read_event().map_err(invalid)? {
            Event::Eof if depth == 0 => return Ok(()),
            Event::Eof => return Err(SidecarError::Invalid("unclosed element".into())),
            Event::DocType(_) => return Err(SidecarError::Doctype),
            Event::Start(start) => {
                depth += 1;
                for attribute in start.attributes() {
                    attribute.map_err(invalid)?;
                }
            }
            Event::Empty(start) => {
                for attribute in start.attributes() {
                    attribute.map_err(invalid)?;
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
}

fn read_existing(path: &Path) -> Result<Option<String>, SidecarError> {
    let bytes = match fsx::read_limited(path, MAX_SIDECAR_BYTES + 1) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(match std::fs::symlink_metadata(path) {
                Ok(metadata) if !metadata.file_type().is_file() => SidecarError::NotRegularFile,
                _ => error.into(),
            });
        }
    };
    if bytes.len() as u64 > MAX_SIDECAR_BYTES {
        return Err(SidecarError::TooLarge);
    }
    String::from_utf8(bytes).map(Some).map_err(invalid)
}

pub fn write(
    path: &Path,
    decision: &Decision,
    previous: &[String],
) -> Result<Vec<String>, SidecarError> {
    let parent = path
        .parent()
        .ok_or_else(|| SidecarError::Io(std::io::Error::other("sidecar has no folder")))?;
    let written: Vec<String> = managed_values(decision)
        .into_iter()
        .map(|(name, _)| name.to_owned())
        .collect();
    let mut retried = false;
    loop {
        let existing = read_existing(path)?;
        let content = match &existing {
            Some(existing) => merge(existing, decision, previous)?,
            None => fresh(decision),
        };
        validate(&content)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".discombobulator-")
            .suffix(".xmp.tmp")
            .tempfile_in(parent)?;
        temporary.write_all(content.as_bytes())?;
        temporary.as_file().sync_all()?;
        if existing.is_some() {
            temporary.persist(path).map_err(|error| error.error)?;
            return Ok(written);
        }
        match temporary.persist_noclobber(path) {
            Ok(_) => return Ok(written),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists && !retried => {
                retried = true;
            }
            Err(error) => return Err(error.error.into()),
        }
    }
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

    fn owned(decision: &Decision) -> Vec<String> {
        managed_values(decision)
            .into_iter()
            .map(|(name, _)| name.to_owned())
            .collect()
    }

    const RDF_OPEN: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">"#;
    const RDF_CLOSE: &str = "</rdf:RDF></x:xmpmeta>";

    fn document(body: &str) -> String {
        format!("{RDF_OPEN}{body}{RDF_CLOSE}")
    }

    #[test]
    fn removes_owned_fields_from_every_description() -> Result<(), SidecarError> {
        let existing = document(
            r#"<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="2"/><rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="3" xmp:Label="Red" xmp:CreatorTool="LR"><xmp:Label>Blue</xmp:Label></rdf:Description>"#,
        );
        let merged = merge(
            &existing,
            &decision(None, false, 5),
            &["xmp:Label".to_owned()],
        )?;
        assert_eq!(merged.matches("Rating").count(), 1);
        assert!(merged.contains(r#"xmp:Rating="5""#));
        assert!(!merged.contains("Label"));
        assert!(merged.contains(r#"xmp:CreatorTool="LR""#));
        Ok(())
    }

    #[test]
    fn matches_fields_by_namespace_not_prefix() -> Result<(), SidecarError> {
        let existing = document(
            r#"<rdf:Description rdf:about="" xmlns:xap="http://ns.adobe.com/xap/1.0/" xmlns:dm="http://ns.adobe.com/xmp/1.0/DynamicMedia/" xap:Rating="4" dm:pick="1"><dm:good>true</dm:good></rdf:Description>"#,
        );
        let merged = merge(&existing, &decision(Some(Mark::Delete), false, 3), &[])?;
        assert!(!merged.contains("xap:Rating"));
        assert!(!merged.contains("dm:pick"));
        assert!(!merged.contains("dm:good"));
        assert!(merged.contains(r#"xmp:Rating="3""#));
        assert!(merged.contains(r#"xmpDM:pick="-1""#));
        Ok(())
    }

    #[test]
    fn uses_a_fresh_prefix_when_xmp_is_taken() -> Result<(), SidecarError> {
        let existing =
            document(r#"<rdf:Description rdf:about="" xmlns:xmp="urn:other" xmp:Rating="keep"/>"#);
        let merged = merge(&existing, &decision(None, false, 2), &[])?;
        assert!(merged.contains(r#"xmp:Rating="keep""#));
        assert!(merged.contains(r#"xmlns:xmp1="http://ns.adobe.com/xap/1.0/""#));
        assert!(merged.contains(r#"xmp1:Rating="2""#));
        Ok(())
    }

    #[test]
    fn requotes_kept_attributes_safely() -> Result<(), SidecarError> {
        let existing = document(
            r#"<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" dc:title='say "hi" &amp; &#10;bye'/>"#,
        );
        let merged = merge(&existing, &decision(Some(Mark::Approved), false, 0), &[])?;
        validate(&merged)?;
        let mut reader = NsReader::from_str(&merged);
        let mut title = None;
        loop {
            match reader.read_event().map_err(invalid)? {
                Event::Empty(start) => {
                    for attribute in start.attributes() {
                        let attribute = attribute.map_err(invalid)?;
                        if attribute.key.as_ref() == "dc:title" {
                            title = Some(
                                attribute
                                    .normalized_value(XmlVersion::Implicit1_0)
                                    .map_err(invalid)?
                                    .into_owned(),
                            );
                        }
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        assert_eq!(title.as_deref(), Some("say \"hi\" & \nbye"));
        Ok(())
    }

    #[test]
    fn keeps_values_the_app_did_not_write() -> Result<(), SidecarError> {
        let existing = document(
            r#"<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="4" xmp:Label="Red"/>"#,
        );
        let approved = decision(Some(Mark::Approved), false, 0);
        let merged = merge(&existing, &approved, &[])?;
        assert!(merged.contains(r#"xmp:Rating="4""#));
        assert!(merged.contains(r#"xmp:Label="Red""#));
        assert!(merged.contains(r#"xmpDM:pick="1""#));
        let cleared = merge(
            &existing,
            &Decision::default(),
            &["xmp:Rating".to_owned(), "xmpDM:pick".to_owned()],
        )?;
        assert!(!cleared.contains("Rating"));
        assert!(cleared.contains(r#"xmp:Label="Red""#));
        Ok(())
    }

    #[test]
    fn refuses_oversized_sidecars() -> Result<(), SidecarError> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("DSCF0001.xmp");
        std::fs::write(&path, vec![b' '; MAX_SIDECAR_BYTES as usize + 1])?;
        assert!(matches!(
            write(&path, &Decision::default(), &[]),
            Err(SidecarError::TooLarge)
        ));
        Ok(())
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
        let approved = decision(Some(Mark::Approved), false, 2);
        let first = fresh(&approved);
        let merged = merge(
            &first,
            &decision(Some(Mark::Delete), false, 0),
            &owned(&approved),
        )?;
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
        let merged = merge(
            existing,
            &decision(Some(Mark::Unsure), false, 1),
            &["xmpDM:pick".to_owned()],
        )?;
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
                &Decision::default(),
                &[]
            ),
            Err(SidecarError::Doctype)
        ));
        assert!(merge("<x:xmpmeta><rdf:Description>", &Decision::default(), &[]).is_err());
        assert!(matches!(
            merge("<x:xmpmeta/>", &Decision::default(), &[]),
            Err(SidecarError::NoDescription)
        ));
    }

    #[test]
    fn writes_new_sidecar_and_updates_it() -> Result<(), SidecarError> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("DSCF0001.xmp");
        let written = write(&path, &decision(Some(Mark::Approved), true, 4), &[])?;
        assert_eq!(
            written,
            ["xmp:Rating", "xmp:Label", "xmpDM:pick", "xmpDM:good"]
        );
        assert!(write(&path, &decision(None, false, 0), &written)?.is_empty());
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
            write(&link, &Decision::default(), &[]),
            Err(SidecarError::NotRegularFile)
        ));
        assert_eq!(std::fs::read_to_string(target)?, "keep");
        Ok(())
    }
}
