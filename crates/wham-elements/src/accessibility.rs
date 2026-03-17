use serde::{Deserialize, Serialize};

use wham_core::types::Rect;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum A11yRole {
    Form,
    Group,
    Label,
    TextBox,
    CheckBox,
    RadioButton,
    Button,
    ComboBox,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct A11yState {
    pub focused: bool,
    pub disabled: bool,
    pub invalid: bool,
    pub required: bool,
    pub expanded: bool,
    pub selected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct A11yNode {
    pub id: u64,
    pub role: A11yRole,
    pub name: String,
    pub value: Option<String>,
    pub bounds: Rect,
    pub state: A11yState,
    pub children: Vec<A11yNode>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct A11yTree {
    pub root: A11yNode,
}

impl A11yTree {
    pub fn flatten(&self) -> Vec<&A11yNode> {
        let mut out = Vec::new();
        fn walk<'a>(node: &'a A11yNode, out: &mut Vec<&'a A11yNode>) {
            out.push(node);
            for child in &node.children {
                walk(child, out);
            }
        }
        walk(&self.root, &mut out);
        out
    }
}
