use serde::{Deserialize, Serialize};

use crate::types::Rect;

/// Per-widget interaction state that is role-independent.
///
/// This struct carries only the boolean flags that apply uniformly to every
/// accessible node regardless of its role.  The ARIA role itself lives in the
/// element layer (`wham-elements`), which parameterises [`A11yNode`] and
/// [`A11yTree`] with a concrete `R`.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct A11yState {
    pub focused: bool,
    pub disabled: bool,
    pub invalid: bool,
    pub required: bool,
    pub expanded: bool,
    pub selected: bool,
}

/// A single node in the accessibility tree, generic over its role type `R`.
///
/// `R` is typically `A11yRole` from `wham-elements`, but keeping it generic
/// allows `wham-core` to remain free of any element-level knowledge.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct A11yNode<R> {
    pub id: u64,
    pub role: R,
    pub name: String,
    pub value: Option<String>,
    pub bounds: Rect,
    pub state: A11yState,
    pub children: Vec<A11yNode<R>>,
}

/// The root of an accessibility tree, generic over role type `R`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct A11yTree<R> {
    pub root: A11yNode<R>,
}

impl<R> A11yTree<R> {
    /// Flatten the tree into a depth-first pre-order list of node references.
    pub fn flatten(&self) -> Vec<&A11yNode<R>> {
        let mut out = Vec::new();
        fn walk<'a, R>(node: &'a A11yNode<R>, out: &mut Vec<&'a A11yNode<R>>) {
            out.push(node);
            for child in &node.children {
                walk(child, out);
            }
        }
        walk(&self.root, &mut out);
        out
    }
}
