#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Can be used as primary agent (default)
    Primary,
    /// Can be used as a subagent spawned by another agent
    Subagent,
    /// Can be used as both primary and subagent
    All,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub description: String,
    pub mode: AgentMode,
    /// Whether this is a native/built-in agent
    pub native: bool,
    /// Hidden from UI agent picker
    pub hidden: bool,
    pub color: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_tokens: Option<u32>,
    pub max_steps: Option<u32>,
    /// Additional system prompt text
    pub prompt: Option<String>,
    /// Tool IDs this agent can use (empty = all)
    pub tools: Vec<String>,
    /// Tool IDs this agent cannot use
    pub disabled_tools: Vec<String>,
    pub permission: Vec<AgentPermissionRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentPermissionRule {
    pub permission: String,
    pub pattern: String,
    pub action: PermissionAction,
}

impl AgentInfo {
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

impl Default for AgentInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            mode: AgentMode::Primary,
            native: false,
            hidden: false,
            color: None,
            model: None,
            temperature: None,
            top_p: None,
            max_tokens: None,
            max_steps: None,
            prompt: None,
            tools: Vec::new(),
            disabled_tools: Vec::new(),
            permission: Vec::new(),
        }
    }
}
