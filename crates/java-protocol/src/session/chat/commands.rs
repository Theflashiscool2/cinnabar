//! Server-owned Brigadier tree and signable argument ranges for the pinned protocol.
#[path = "commands/parse.rs"]
mod parse;
use crate::{ProtocolError as Error, target, wire::Reader};
#[derive(Clone, Debug)]
pub(super) enum Kind {
    Root,
    Literal(String),
    Argument {
        name: String,
        parser: &'static str,
        properties: Properties,
        signable: bool,
    },
}
#[derive(Clone, Debug)]
#[allow(dead_code)] // Preserve unimplemented native parser bounds without approximating their syntax.
pub(super) enum Properties {
    None,
    Byte(u8),
    String(i32),
    Number {
        lower: Option<Vec<u8>>,
        upper: Option<Vec<u8>>,
    },
    Minimum(i32),
    Registry(String),
}
#[derive(Clone, Debug)]
pub(super) struct Node {
    kind: Kind,
    children: Vec<usize>,
    redirect: Option<usize>,
    _flags: u8,
    may_sign: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Tree {
    nodes: Vec<Node>,
    root: usize,
}
impl Tree {
    /// Every native parser's properties are consumed, including parsers not needed by
    /// ordinary player chat. Unknown parser identities cannot be skipped without framing.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        let count = r.count(r.0.len() / 2)?;
        let mut nodes = Vec::with_capacity(count);
        for _ in 0..count {
            let flags = r.byte()?;
            let n = r.count(r.0.len())?;
            let mut children = Vec::with_capacity(n);
            for _ in 0..n {
                children.push(index(r.var()?, count)?);
            }
            let redirect = if flags & 8 != 0 {
                Some(index(r.var()?, count)?)
            } else {
                None
            };
            let kind = match flags & 3 {
                0 => Kind::Root,
                1 => Kind::Literal(r.string(32767)?),
                2 => {
                    let name = r.string(32767)?;
                    let id = usize::try_from(r.var()?)
                        .map_err(|_| Error::Invalid("Java command parser index"))?;
                    let &(parser, shape, signable) = target::COMMAND_ARGUMENTS
                        .get(id)
                        .ok_or(Error::Invalid("unknown Java command parser"))?;
                    let properties = match shape {
                        "none" => Properties::None,
                        "byte" => Properties::Byte(r.byte()?),
                        "string" => Properties::String(r.var()?),
                        "int" => Properties::Minimum(r.i32()?),
                        "registry" => Properties::Registry(r.string(32767)?),
                        "float" | "double" | "integer" | "long" => {
                            let flags = r.byte()?;
                            let bytes = if matches!(shape, "double" | "long") {
                                8
                            } else {
                                4
                            };
                            Properties::Number {
                                lower: if flags & 1 != 0 {
                                    Some(r.take(bytes)?.to_vec())
                                } else {
                                    None
                                },
                                upper: if flags & 2 != 0 {
                                    Some(r.take(bytes)?.to_vec())
                                } else {
                                    None
                                },
                            }
                        }
                        _ => return Err(Error::Invalid("Java command property schema")),
                    };
                    if flags & 16 != 0 {
                        r.string(32767)?;
                    }
                    Kind::Argument {
                        name,
                        parser,
                        properties,
                        signable,
                    }
                }
                _ => return Err(Error::Invalid("Java command node type")),
            };
            let may_sign = matches!(kind, Kind::Argument { signable: true, .. });
            nodes.push(Node {
                kind,
                children,
                redirect,
                _flags: flags,
                may_sign,
            });
        }
        let root = index(r.var()?, count)?;
        if !matches!(nodes[root].kind, Kind::Root) {
            return Err(Error::Invalid("Java command root"));
        }
        validate_graph(&nodes, false)?;
        validate_graph(&nodes, true)?;
        // Reverse edges handle redirects/cycles without recursive graph walks.
        let mut reverse = vec![Vec::new(); count];
        for (parent, node) in nodes.iter().enumerate() {
            for &child in &node.children {
                reverse[child].push(parent);
            }
            if let Some(target) = node.redirect
                && target != root
            {
                reverse[target].push(parent);
            }
        }
        let mut queue: std::collections::VecDeque<_> = nodes
            .iter()
            .enumerate()
            .filter_map(|(id, n)| n.may_sign.then_some(id))
            .collect();
        while let Some(child) = queue.pop_front() {
            for &parent in &reverse[child] {
                if !nodes[parent].may_sign {
                    nodes[parent].may_sign = true;
                    queue.push_back(parent);
                }
            }
        }
        Ok(Self { nodes, root })
    }
    pub fn counts(&self) -> (usize, usize) {
        (
            self.nodes.len(),
            self.nodes
                .iter()
                .filter(|n| matches!(n.kind, Kind::Argument { signable: true, .. }))
                .count(),
        )
    }
}
fn index(value: i32, count: usize) -> Result<usize, Error> {
    usize::try_from(value)
        .ok()
        .filter(|&i| i < count)
        .ok_or(Error::Invalid("Java command node index"))
}

#[cfg(test)]
#[path = "commands/tests.rs"]
mod tests;

/// Native validates child dependencies and redirect dependencies separately. A redirect
/// back to the root is legal; a cycle entirely within either dependency graph is not.
fn validate_graph(nodes: &[Node], redirects: bool) -> Result<(), Error> {
    let mut degree = vec![0usize; nodes.len()];
    let mut reverse = vec![Vec::new(); nodes.len()];
    for (id, node) in nodes.iter().enumerate() {
        let edges: Vec<_> = if redirects {
            node.redirect.iter().copied().collect()
        } else {
            node.children.clone()
        };
        degree[id] = edges.len();
        for child in edges {
            reverse[child].push(id);
        }
    }
    let mut queue: std::collections::VecDeque<_> = degree
        .iter()
        .enumerate()
        .filter_map(|(id, &n)| (n == 0).then_some(id))
        .collect();
    let mut visited = 0;
    while let Some(child) = queue.pop_front() {
        visited += 1;
        for &parent in &reverse[child] {
            degree[parent] -= 1;
            if degree[parent] == 0 {
                queue.push_back(parent);
            }
        }
    }
    if visited == nodes.len() {
        Ok(())
    } else {
        Err(Error::Invalid("cyclic Java command dependency"))
    }
}
