use crate::{
    AgentToolDefinition, AgentToolName, AgentToolParameterDefinition, AgentToolParameterKind,
};

pub(crate) fn definition(
    tool: AgentToolName,
    wire_name: &str,
    description: &str,
    parameters: Vec<AgentToolParameterDefinition>,
) -> AgentToolDefinition {
    AgentToolDefinition {
        tool,
        wire_name: wire_name.to_owned(),
        description: description.to_owned(),
        parameters,
    }
}

pub(crate) fn text(name: &str, description: &str, required: bool) -> AgentToolParameterDefinition {
    AgentToolParameterDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        kind: AgentToolParameterKind::Text,
        required,
    }
}

pub(crate) fn integer(
    name: &str,
    description: &str,
    minimum: i64,
    maximum: i64,
    required: bool,
) -> AgentToolParameterDefinition {
    AgentToolParameterDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        kind: AgentToolParameterKind::Integer { minimum, maximum },
        required,
    }
}

pub(crate) fn boolean(
    name: &str,
    description: &str,
    required: bool,
) -> AgentToolParameterDefinition {
    AgentToolParameterDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        kind: AgentToolParameterKind::Boolean,
        required,
    }
}

pub(crate) fn text_list(
    name: &str,
    description: &str,
    maximum_items: u16,
    required: bool,
) -> AgentToolParameterDefinition {
    AgentToolParameterDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        kind: AgentToolParameterKind::TextList { maximum_items },
        required,
    }
}

pub(crate) fn decimal_permille(
    name: &str,
    description: &str,
    minimum: u16,
    maximum: u16,
    required: bool,
) -> AgentToolParameterDefinition {
    AgentToolParameterDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        kind: AgentToolParameterKind::DecimalPermille { minimum, maximum },
        required,
    }
}
