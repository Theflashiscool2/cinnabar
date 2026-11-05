//! Brigadier relevant-child selection and redirect parsing adapted from the MIT source
//! pinned in third_party/java-command-parser/UPSTREAM.json. See its retained license.
//! Only parsers implemented exactly here may select a signature range. Other commands
//! remain server-parsed; unsupported signable prefixes never advance a signing chain.
use super::{Kind, Properties, Tree};
use crate::ProtocolError as Error;
#[derive(Clone)]
struct Parsed {
    cursor: usize,
    arguments: Vec<(String, String)>,
    failed: bool,
}
impl Tree {
    pub fn arguments(&self, input: &str, permission: u8) -> Result<Vec<(String, String)>, Error> {
        if input.encode_utf16().count() > super::super::PLAYER_MAX_UTF16 {
            return Err(Error::Invalid(
                "Java command exceeds native chat editor length",
            ));
        }
        let parsed = self.parse(self.root, input, 0, permission, &mut Vec::new())?;
        if parsed.arguments.len() > 8 {
            return Err(Error::Invalid("Java signed argument count"));
        }
        Ok(parsed.arguments)
    }
    fn parse(
        &self,
        parent: usize,
        input: &str,
        cursor: usize,
        permission: u8,
        path: &mut Vec<(usize, usize)>,
    ) -> Result<Parsed, Error> {
        let empty = || Parsed {
            cursor,
            arguments: Vec::new(),
            failed: true,
        };
        if path.contains(&(parent, cursor)) {
            return Ok(empty());
        }
        path.push((parent, cursor));
        let result = self.parse_children(parent, input, cursor, permission, path);
        path.pop();
        result
    }
    fn parse_children(
        &self,
        parent: usize,
        input: &str,
        cursor: usize,
        permission: u8,
        path: &mut Vec<(usize, usize)>,
    ) -> Result<Parsed, Error> {
        let children = &self.nodes[parent].children;
        let word = input[cursor..].split(' ').next().unwrap_or("");
        let literal = children
            .iter()
            .copied()
            .find(|&child| matches!(&self.nodes[child].kind, Kind::Literal(name) if name == word));
        let relevant: Vec<_> = if let Some(literal) = literal {
            vec![literal]
        } else {
            children
                .iter()
                .copied()
                .filter(|&child| matches!(self.nodes[child].kind, Kind::Argument { .. }))
                .collect()
        };
        // SignableCommand's visitor does not follow a redirect back to the initial root.
        // A branch with no such arguments needs no local argument parsing at all.
        if relevant.iter().all(|&child| !self.nodes[child].may_sign) {
            return Ok(Parsed {
                cursor: input.len(),
                arguments: Vec::new(),
                failed: false,
            });
        }
        let mut potentials = Vec::new();
        for child in relevant {
            let node = &self.nodes[child];
            let (end, argument) = match &node.kind {
                Kind::Root => continue,
                Kind::Literal(name) => (cursor + name.len(), None),
                Kind::Argument {
                    name,
                    parser,
                    properties,
                    signable,
                } => {
                    let Some(end) = argument_end(input, cursor, parser, properties, permission)?
                    else {
                        continue;
                    };
                    (
                        end,
                        signable.then(|| (name.clone(), input[cursor..end].to_owned())),
                    )
                }
            };
            if end < input.len() && input.as_bytes()[end] != b' ' {
                continue;
            }
            let mut arguments: Vec<_> = argument.into_iter().collect();
            let remaining = input.len() - end;
            if remaining >= if node.redirect.is_some() { 1 } else { 2 } {
                let next = end + 1;
                if let Some(target) = node.redirect {
                    if target == self.root {
                        return Ok(Parsed {
                            cursor: input.len(),
                            arguments,
                            failed: false,
                        });
                    }
                    let parsed = self.parse(target, input, next, permission, path)?;
                    arguments.extend(parsed.arguments);
                    return Ok(Parsed {
                        cursor: parsed.cursor,
                        arguments,
                        failed: parsed.failed,
                    });
                }
                let mut parsed = self.parse(child, input, next, permission, path)?;
                arguments.append(&mut parsed.arguments);
                potentials.push(Parsed {
                    cursor: parsed.cursor,
                    arguments,
                    failed: parsed.failed,
                });
            } else {
                potentials.push(Parsed {
                    cursor: end,
                    arguments,
                    failed: false,
                });
            }
        }
        // Brigadier uses stable ordering: fully consumed input, then no exceptions.
        potentials.sort_by_key(|p| (p.cursor < input.len(), p.failed));
        Ok(potentials.into_iter().next().unwrap_or(Parsed {
            cursor,
            arguments: Vec::new(),
            failed: true,
        }))
    }
}
fn argument_end(
    input: &str,
    cursor: usize,
    parser: &str,
    properties: &Properties,
    permission: u8,
) -> Result<Option<usize>, Error> {
    let rest = &input[cursor..];
    match parser {
        "minecraft:message" => {
            if permission >= 2 {
                // Normal text and selectors without options have an exact range. Complex
                // selector validation needs the remaining native selector parser; never
                // sign a guessed range and desynchronize the account's message index.
                for (at, _) in rest.match_indices('@') {
                    let tail = &rest[at + 1..];
                    if tail.starts_with(['p', 'a', 'r', 's', 'e', 'n'])
                        && tail.as_bytes().get(1) == Some(&b'[')
                    {
                        return Err(Error::UnsupportedCommandArgument(
                            "complex selector in a signed Java command",
                        ));
                    }
                }
            }
            Ok(Some(input.len()))
        }
        "brigadier:string" => match properties {
            Properties::String(0) => Ok(Some(cursor + unquoted(rest))),
            Properties::String(1) => Ok(read_string(rest).map(|(_, end)| cursor + end)),
            Properties::String(2) => Ok(Some(input.len())),
            _ => Err(Error::Invalid("Java command string property")),
        },
        "minecraft:entity" | "minecraft:game_profile" => {
            if rest.starts_with('@') {
                if permission < 2 {
                    return Ok(None);
                }
                let single = matches!(properties, Properties::Byte(flags) if flags & 1 != 0);
                let players = parser == "minecraft:game_profile"
                    || matches!(properties, Properties::Byte(flags) if flags & 2 != 0);
                let selector = rest.as_bytes().get(1).copied();
                if rest.as_bytes().get(2) == Some(&b'[') {
                    return Err(Error::UnsupportedCommandArgument(
                        "complex signed Java command target",
                    ));
                }
                if !matches!(selector, Some(b'p' | b'a' | b'r' | b's' | b'e' | b'n')) {
                    return Ok(None);
                }
                if single && matches!(selector, Some(b'a' | b'e'))
                    || players && matches!(selector, Some(b'e' | b'n'))
                    || parser == "minecraft:game_profile" && selector == Some(b's')
                {
                    return Ok(None);
                }
                return Ok(Some(cursor + 2));
            }
            if parser == "minecraft:game_profile" {
                return Ok(Some(cursor + rest.find(' ').unwrap_or(rest.len())));
            }
            let Some((name, bytes)) = read_string(rest) else {
                return Ok(None);
            };
            if name.chars().filter(|&c| c == '-').count() == 4 {
                return Err(Error::UnsupportedCommandArgument(
                    "UUID signed Java command target",
                ));
            }
            Ok((!name.is_empty() && name.encode_utf16().count() <= 16).then_some(cursor + bytes))
        }
        "brigadier:bool" => {
            let Some((value, end)) = read_string(rest) else {
                return Ok(None);
            };
            Ok(matches!(value.as_str(), "true" | "false").then_some(cursor + end))
        }
        _ => Err(Error::UnsupportedCommandArgument(
            "argument parser before a signed Java command message",
        )),
    }
}
fn unquoted(text: &str) -> usize {
    text.bytes()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'+'))
        .count()
}
fn read_string(text: &str) -> Option<(String, usize)> {
    let Some(quote @ ('\'' | '"')) = text.chars().next() else {
        let end = unquoted(text);
        return Some((text[..end].to_owned(), end));
    };
    let mut value = String::new();
    let mut escaped = false;
    for (at, ch) in text.char_indices().skip(1) {
        if escaped {
            if ch != quote && ch != '\\' {
                return None;
            }
            value.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some((value, at + ch.len_utf8()));
        } else {
            value.push(ch);
        }
    }
    None
}
