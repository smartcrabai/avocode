use crate::tool::schema::Tool;

pub struct ToolRegistry {
    tools: std::collections::HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.id().to_owned(), tool);
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&dyn Tool> {
        self.tools.get(id).map(std::convert::AsRef::as_ref)
    }

    #[must_use]
    pub fn list(&self) -> Vec<&dyn Tool> {
        self.tools
            .values()
            .map(std::convert::AsRef::as_ref)
            .collect()
    }

    #[must_use]
    pub fn to_definitions(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "name": t.id(),
                    "description": t.description(),
                    "input_schema": t.parameters_schema(),
                })
            })
            .collect()
    }

    #[must_use]
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.register(Box::new(crate::tool::bash::BashTool));
        r.register(Box::new(crate::tool::read::ReadTool));
        r.register(Box::new(crate::tool::write::WriteTool));
        r.register(Box::new(crate::tool::edit::EditTool));
        r.register(Box::new(crate::tool::glob::GlobTool));
        r.register(Box::new(crate::tool::grep::GrepTool));
        r.register(Box::new(crate::tool::ls::LsTool));
        r.register(Box::new(crate::tool::webfetch::WebFetchTool));
        r
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_builtins_contains_all_tools() {
        let registry = ToolRegistry::with_builtins();
        for name in ["bash", "read", "write", "edit", "glob", "grep", "ls"] {
            assert!(registry.get(name).is_some(), "missing tool: {name}");
        }
    }
}
