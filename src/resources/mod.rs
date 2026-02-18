use rmcp::model::{Annotated, RawResource, ReadResourceResult, Resource, ResourceContents};

// Trait and provider-based resource registry (similar to prompts)
pub trait ResourceProvider {
    /// Get the resource URI
    fn uri(&self) -> &'static str;

    /// Get the resource name
    fn name(&self) -> &'static str;

    /// Get the resource description
    fn description(&self) -> &'static str;

    /// Get the resource MIME type
    fn mime_type(&self) -> &'static str;

    /// Get the resource content
    fn content(&self) -> String;

    /// Get the resource metadata
    fn meta(&self) -> Resource {
        let size = self.content().len() as u32;
        let raw = RawResource {
            size: Some(size),
            uri: self.uri().to_string(),
            name: self.name().to_string(),
            title: Some(self.name().to_string()),
            mime_type: Some(self.mime_type().to_string()),
            description: Some(self.description().to_string()),
            icons: None,
            meta: None,
        };
        Annotated::new(raw, None)
    }

    fn read(&self) -> ReadResourceResult {
        ReadResourceResult {
            contents: vec![ResourceContents::text(self.content(), self.uri())],
        }
    }
}

// Instructions resource
pub struct InstructionsResource;

impl ResourceProvider for InstructionsResource {
    fn uri(&self) -> &'static str {
        "surrealmcp://instructions"
    }

    fn name(&self) -> &'static str {
        "SurrealMCP Instructions"
    }

    fn mime_type(&self) -> &'static str {
        "text/markdown"
    }

    fn description(&self) -> &'static str {
        "Full instructions and guidelines for the SurrealDB MCP server"
    }

    fn content(&self) -> String {
        include_str!("../../instructions.md").to_string()
    }
}

/// Registry of all available resources
pub struct ResourceRegistry;

impl ResourceRegistry {
    /// Get all available resource providers
    pub fn get_providers() -> Vec<Box<dyn ResourceProvider>> {
        vec![Box::new(InstructionsResource)]
    }

    /// Find a resource provider by URI
    pub fn find_by_uri(uri: &str) -> Option<Box<dyn ResourceProvider>> {
        Self::get_providers().into_iter().find(|p| p.uri() == uri)
    }
}

/// List all available resources
pub fn list_resources() -> Vec<Resource> {
    ResourceRegistry::get_providers()
        .into_iter()
        .map(|p| p.meta())
        .collect()
}

pub fn read_resource(uri: &str) -> Option<ReadResourceResult> {
    ResourceRegistry::find_by_uri(uri).map(|provider| provider.read())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instructions_resource() {
        let resource = InstructionsResource;
        assert_eq!(resource.uri(), "surrealmcp://instructions");
        assert_eq!(resource.name(), "SurrealMCP Instructions");
        assert_eq!(resource.mime_type(), "text/markdown");
        assert!(!resource.content().is_empty());

        let meta = resource.meta();
        assert_eq!(meta.uri, "surrealmcp://instructions");
        assert_eq!(meta.name, "SurrealMCP Instructions");
    }

    #[test]
    fn test_resource_registry() {
        let providers = ResourceRegistry::get_providers();
        assert!(!providers.is_empty());
        assert_eq!(providers[0].uri(), "surrealmcp://instructions");

        let found = ResourceRegistry::find_by_uri("surrealmcp://instructions");
        assert!(found.is_some());
        assert_eq!(found.unwrap().uri(), "surrealmcp://instructions");

        let not_found = ResourceRegistry::find_by_uri("surrealmcp://non-existent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_list_resources() {
        let resources = list_resources();
        assert!(!resources.is_empty());
        assert_eq!(resources[0].uri, "surrealmcp://instructions");
    }

    #[test]
    fn test_read_resource() {
        let result = read_resource("surrealmcp://instructions");
        assert!(result.is_some());
        let contents = result.unwrap().contents;
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].uri, "surrealmcp://instructions");
    }
}
