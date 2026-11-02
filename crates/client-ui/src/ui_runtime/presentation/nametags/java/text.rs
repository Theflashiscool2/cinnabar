//! Literal Java components retained as bounded shared-atlas formatting, with explicit gaps.
use std::sync::Arc;
use world::{NbtValue, java::roster::teams::Parameters};

#[derive(Clone, Copy, Default)]
struct Style {
    color: Option<char>,
    bold: bool,
    italic: bool,
    obfuscated: bool,
}

pub(super) fn player(name: &str, team: Option<&Parameters>) -> Result<Arc<str>, ()> {
    let style = Style {
        color: team.and_then(|t| t.color),
        ..Default::default()
    };
    let mut out = String::new();
    let mut remaining = ui::MAX_TEXT_SPANS;
    if let Some(t) = team {
        component(&t.prefix, style, &mut out, &mut remaining, 0)?;
    }
    literal(name, style, &mut out)?;
    if let Some(t) = team {
        component(&t.suffix, style, &mut out, &mut remaining, 0)?;
    }
    Ok(out.into())
}
fn literal(s: &str, style: Style, out: &mut String) -> Result<(), ()> {
    // Reset sibling formatting before restoring inherited fields. Bedrock colour codes alone
    // do not reset bold/italic, so simply concatenating component strings is incorrect.
    let mut codes = String::from("§r");
    for code in [
        style.color,
        style.bold.then_some('l'),
        style.italic.then_some('o'),
        style.obfuscated.then_some('k'),
    ]
    .into_iter()
    .flatten()
    {
        codes.push('§');
        codes.push(code);
    }
    if out
        .len()
        .saturating_add(codes.len())
        .saturating_add(s.len())
        > ui::UiLimits::MAX_TEXT_BYTES
    {
        return Err(());
    }
    out.push_str(&codes);
    out.push_str(s);
    Ok(())
}
fn component(
    v: &NbtValue,
    inherited: Style,
    out: &mut String,
    remaining: &mut usize,
    depth: usize,
) -> Result<(), ()> {
    if depth > world::MAX_NBT_DEPTH || *remaining == 0 {
        return Err(());
    }
    *remaining -= 1;
    match v {
        NbtValue::String(s) => literal(s, inherited, out),
        NbtValue::List(parts) => {
            // Native list components inherit the first element's style for subsequent siblings.
            let mut style = inherited;
            for (i, part) in parts.iter().enumerate() {
                if i == 0 {
                    style = styled(part, inherited)?;
                }
                component(part, style, out, remaining, depth + 1)?;
            }
            Ok(())
        }
        NbtValue::Compound(c) => {
            let style = styled(v, inherited)?;
            let Some(NbtValue::String(s)) = c.get("text") else {
                return Err(());
            };
            literal(s, style, out)?;
            if let Some(extra) = c.get("extra") {
                let NbtValue::List(parts) = extra else {
                    return Err(());
                };
                for part in parts {
                    component(part, style, out, remaining, depth + 1)?;
                }
            }
            Ok(())
        }
        _ => Err(()),
    }
}
fn styled(v: &NbtValue, mut s: Style) -> Result<Style, ()> {
    let NbtValue::Compound(c) = v else {
        return Ok(s);
    };
    if c.boolean("underlined") == Some(true)
        || c.boolean("strikethrough") == Some(true)
        || c.get("font").is_some()
        || c.get("shadow_color").is_some()
    {
        return Err(());
    }
    if let Some(color) = c.get("color") {
        let NbtValue::String(name) = color else {
            return Err(());
        };
        s.color = Some(
            ui::BedrockColor::from_name(name)
                .and_then(|c| c.descriptor())
                .filter(|c| c.code.is_ascii_hexdigit())
                .ok_or(())?
                .code,
        );
    }
    s.bold = c.boolean("bold").unwrap_or(s.bold);
    s.italic = c.boolean("italic").unwrap_or(s.italic);
    s.obfuscated = c.boolean("obfuscated").unwrap_or(s.obfuscated);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compound(fields: impl IntoIterator<Item = (&'static str, NbtValue)>) -> NbtValue {
        let mut c = world::NbtCompound::default();
        for (k, v) in fields {
            c.insert(k, v);
        }
        NbtValue::Compound(c)
    }
    #[test]
    fn inherited_styles_restore_after_siblings_and_unicode_is_retained() {
        let v = compound([
            ("text", NbtValue::String("名".into())),
            ("color", NbtValue::String("red".into())),
            ("bold", NbtValue::Byte(1)),
            (
                "extra",
                NbtValue::List(vec![
                    compound([
                        ("text", NbtValue::String("A".into())),
                        ("bold", NbtValue::Byte(0)),
                    ]),
                    NbtValue::String("𐐀".into()),
                ]),
            ),
        ]);
        let mut out = String::new();
        let mut remaining = ui::MAX_TEXT_SPANS;
        component(&v, Style::default(), &mut out, &mut remaining, 0).unwrap();
        assert_eq!(out, "§r§c§l名§r§cA§r§c§l𐐀");
    }
    #[test]
    fn unsupported_content_and_unbounded_text_are_explicit() {
        let mut out = String::new();
        let mut nodes = 4;
        assert!(
            component(
                &compound([("translate", NbtValue::String("key".into()))]),
                Style::default(),
                &mut out,
                &mut nodes,
                0
            )
            .is_err()
        );
        assert!(player(&"X".repeat(ui::UiLimits::MAX_TEXT_BYTES), None).is_err());
        assert!(
            component(
                &NbtValue::String("A".into()),
                Style::default(),
                &mut out,
                &mut 0,
                0
            )
            .is_err()
        );
    }
}
