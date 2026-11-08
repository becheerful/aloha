use std::collections::HashMap;

use xmpkit::{XmpValue, ns::DC};

use crate::error::AlohaResult;

/// Optional metadata associated with an [`AlohaImage`].
///
/// All fields are taken from XMP Dublin Core schema.
/// This metadata is serialized as XML within the `.aloha` container.
/// All fields are optional and omitted from the serialized output when `None`.
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    /// Contributors to the resource (other than the authors).
    pub contributors: Option<XmpValue /* Array(String) */>,
    /// The extent or scope of the resource.
    pub coverage: Option<XmpValue /* String */>,
    /// The authors of the resource (listed in order of precedence, if significant).
    pub creators: Option<XmpValue /* Array(String) */>,
    /// Date(s) that something interesting happened to the resource.
    pub dates: Option<XmpValue /* Array(DateTime) */>,
    /// A textual description of the content of the resource. Multiple values may be
    /// present for different languages.
    pub description: Option<XmpValue /* Structure(String, XmpValue) */>,
    /// The file format used when saving the resource. Tools and applications should
    /// set this property to the save format of the data. It may include appropriate
    /// qualifiers.
    pub format: Option<XmpValue /* String */>,
    /// Unique identifier of the resource, represented as a version 7 UUID.
    pub identifier: Option<XmpValue /* String */>,
    /// An unordered array specifying the languages used in the resource.
    pub language: Option<XmpValue /* Array(String) */>,
    /// Publishers.
    pub publishers: Option<XmpValue /* Array(String) */>,
    /// Relationships to other documents.
    pub relations: Option<XmpValue /* Array(String) */>,
    /// Informal rights statement, selected by language.
    pub rights: Option<XmpValue /* Structure(String, XmpValue) */>,
    /// Unique identifier of the work from which this resource was derived.
    pub source: Option<XmpValue /* String */>,
    /// An unordered array of descriptive phrases or keywords that specify the topic
    /// of the content of the resource.
    pub subject: Option<XmpValue /* Array(String) */>,
    /// The title of the document, or the name given to the resource. Typically, it
    /// will be a name by which the resource is formally known.
    pub title: Option<XmpValue /* Structure(String, XmpValue) */>,
    /// A document type; for example, novel, poem, or working paper.
    pub rtype: Option<XmpValue /* Array(String) */>,
}

impl Metadata {
    pub fn new() -> Self {
        Self::default().set_identifier(format!("urn:uuid:{}", uuid::Uuid::now_v7()))
    }

    pub fn set_contributors(mut self, value: &[impl Into<String> + Clone]) -> Self {
        self.contributors = Some(XmpValue::Array(
            value
                .iter()
                .map(|s| XmpValue::String(s.clone().into()))
                .collect(),
        ));
        self
    }

    pub fn set_coverage(mut self, value: impl Into<String>) -> Self {
        self.coverage = Some(XmpValue::String(value.into()));
        self
    }

    pub fn set_creators(mut self, value: &[impl Into<String> + Clone]) -> Self {
        self.creators = Some(XmpValue::Array(
            value
                .iter()
                .map(|s| XmpValue::String(s.clone().into()))
                .collect(),
        ));
        self
    }

    pub fn set_date(mut self, value: &[chrono::DateTime<chrono::Utc>]) -> Self {
        self.dates = Some(XmpValue::Array(
            value
                .iter()
                .map(|d| XmpValue::DateTime(d.format("%Y-%m-%d").to_string()))
                .collect(),
        ));
        self
    }

    pub fn set_date_now(mut self) -> Self {
        self.dates = Some(XmpValue::Array(vec![XmpValue::String(
            chrono::Utc::now().format("%Y-%m-%d").to_string(),
        )]));
        self
    }

    pub fn set_description(mut self, value: HashMap<String, String>) -> Self {
        self.description = Some(XmpValue::Structure(
            value
                .into_iter()
                .map(|(k, v)| (k, XmpValue::String(v)))
                .collect(),
        ));
        self
    }

    pub fn set_format(mut self, value: impl Into<String>) -> Self {
        self.format = Some(XmpValue::String(value.into()));
        self
    }

    pub fn set_identifier(mut self, value: impl Into<String>) -> Self {
        self.identifier = Some(XmpValue::String(value.into()));
        self
    }

    pub fn set_language(mut self, value: &[impl Into<String> + Clone]) -> Self {
        self.language = Some(XmpValue::Array(
            value
                .iter()
                .map(|s| XmpValue::String(s.clone().into()))
                .collect(),
        ));
        self
    }

    pub fn set_publishers(mut self, value: &[impl Into<String> + Clone]) -> Self {
        self.publishers = Some(XmpValue::Array(
            value
                .iter()
                .map(|s| XmpValue::String(s.clone().into()))
                .collect(),
        ));
        self
    }

    pub fn set_relations(mut self, value: &[impl Into<String> + Clone]) -> Self {
        self.relations = Some(XmpValue::Array(
            value
                .iter()
                .map(|s| XmpValue::String(s.clone().into()))
                .collect(),
        ));
        self
    }

    pub fn set_rights(mut self, value: HashMap<String, String>) -> Self {
        self.rights = Some(XmpValue::Structure(
            value
                .into_iter()
                .map(|(k, v)| (k, XmpValue::String(v)))
                .collect(),
        ));
        self
    }

    pub fn set_source(mut self, value: impl Into<String>) -> Self {
        self.source = Some(XmpValue::String(value.into()));
        self
    }

    pub fn set_subject(mut self, value: &[impl Into<String> + Clone]) -> Self {
        self.subject = Some(XmpValue::Array(
            value
                .iter()
                .map(|s| XmpValue::String(s.clone().into()))
                .collect(),
        ));
        self
    }

    pub fn set_title(mut self, value: HashMap<String, String>) -> Self {
        self.title = Some(XmpValue::Structure(
            value
                .into_iter()
                .map(|(k, v)| (k, XmpValue::String(v)))
                .collect(),
        ));
        self
    }

    pub fn set_type(mut self, value: &[impl Into<String> + Clone]) -> Self {
        self.rtype = Some(XmpValue::Array(
            value
                .iter()
                .map(|s| XmpValue::String(s.clone().into()))
                .collect(),
        ));
        self
    }

    pub fn to_xmp(&self) -> AlohaResult<String> {
        let mut meta = xmpkit::XmpMeta::new();

        if let Some(contributors) = &self.contributors {
            meta.set_property(DC, "contributor", contributors.to_owned())?;
        }

        if let Some(coverage) = &self.coverage {
            meta.set_property(DC, "coverage", coverage.to_owned())?;
        }

        if let Some(creators) = &self.creators {
            meta.set_property(DC, "creator", creators.to_owned())?;
        }

        if let Some(dates) = &self.dates {
            meta.set_property(DC, "date", dates.to_owned())?;
        }

        if let Some(description) = &self.description {
            meta.set_property(DC, "description", description.to_owned())?;
        }

        if let Some(format) = &self.format {
            meta.set_property(DC, "format", format.to_owned())?;
        }

        if let Some(identifier) = &self.identifier {
            meta.set_property(DC, "identifier", identifier.to_owned())?;
        }

        if let Some(language) = &self.language {
            meta.set_property(DC, "language", language.to_owned())?;
        }

        if let Some(publishers) = &self.publishers {
            meta.set_property(DC, "publisher", publishers.to_owned())?;
        }

        if let Some(relations) = &self.relations {
            meta.set_property(DC, "relation", relations.to_owned())?;
        }

        if let Some(rights) = &self.rights {
            meta.set_property(DC, "rights", rights.to_owned())?;
        }

        if let Some(source) = &self.source {
            meta.set_property(DC, "source", source.to_owned())?;
        }

        if let Some(subject) = &self.subject {
            meta.set_property(DC, "subject", subject.to_owned())?;
        }

        if let Some(title) = &self.title {
            meta.set_property(DC, "title", title.to_owned())?;
        }

        if let Some(rtype) = &self.rtype {
            meta.set_property(DC, "type", rtype.to_owned())?;
        }

        Ok(meta.serialize()?)
    }

    pub fn from_xmp(xmp_str: &str) -> AlohaResult<Self> {
        let meta = xmpkit::XmpMeta::parse(xmp_str)?;

        let contributors = meta.get_property(DC, "contributor");
        let coverage = meta.get_property(DC, "coverage");
        let creators = meta.get_property(DC, "creator");
        let dates = meta.get_property(DC, "date");
        let description = meta.get_property(DC, "description");
        let format = meta.get_property(DC, "format");
        let identifier = meta.get_property(DC, "identifier");
        let language = meta.get_property(DC, "language");
        let publishers = meta.get_property(DC, "publisher");
        let relations = meta.get_property(DC, "relation");
        let rights = meta.get_property(DC, "rights");
        let source = meta.get_property(DC, "source");
        let subject = meta.get_property(DC, "subject");
        let title = meta.get_property(DC, "title");
        let rtype = meta.get_property(DC, "type");

        Ok(Self {
            contributors,
            coverage,
            creators,
            dates,
            description,
            format,
            identifier,
            language,
            publishers,
            relations,
            rights,
            source,
            subject,
            title,
            rtype,
        })
    }
}
