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
        component(&t.prefix, style, &mut out, &mut remaining, 0, None)?;
    }
    literal(name, style, &mut out)?;
    if let Some(t) = team {
        component(&t.suffix, style, &mut out, &mut remaining, 0, None)?;
    }
    Ok(out.into())
}
/// Formats a literal component with optional team decoration using the shared atlas contract.
pub(crate) fn component_text(value: &NbtValue, team: Option<&Parameters>) -> Result<Arc<str>, ()> {
    let style = Style {
        color: team.and_then(|t| t.color),
        ..Default::default()
    };
    let mut out = String::new();
    let mut remaining = ui::MAX_TEXT_SPANS;
    if let Some(team) = team {
        component(&team.prefix, style, &mut out, &mut remaining, 0, None)?;
    }
    component(value, style, &mut out, &mut remaining, 0, None)?;
    if let Some(team) = team {
        component(&team.suffix, style, &mut out, &mut remaining, 0, None)?;
    }
    Ok(out.into())
}

/// Native literal/translated system components, sharing existing styles and atlas bounds.
pub(crate) fn translated_component_text(
    value: &NbtValue,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Arc<str>, ()> {
    let mut out = String::new();
    let mut remaining = ui::MAX_TEXT_SPANS;
    component(
        value,
        Style::default(),
        &mut out,
        &mut remaining,
        0,
        Some(lookup),
    )?;
    Ok(out.into())
}
/// Bounded chat-only content projection when its presentation cannot be represented.
/// Callers must disclose the styling gap. Unknown content still fails; no selector,
/// score, NBT or click action is evaluated and no arbitrary subtree is stringified.
pub(crate) fn translated_component_content(
    value: &NbtValue,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Arc<str>, ()> {
    let mut out = String::new();
    let mut remaining = ui::MAX_TEXT_SPANS;
    component_mode(
        value,
        Style::default(),
        &mut out,
        &mut remaining,
        0,
        Some(lookup),
        true,
    )?;
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
    translate: Option<&dyn Fn(&str) -> Option<String>>,
) -> Result<(), ()> {
    component_mode(v, inherited, out, remaining, depth, translate, false)
}
fn component_mode(
    v: &NbtValue,
    inherited: Style,
    out: &mut String,
    remaining: &mut usize,
    depth: usize,
    translate: Option<&dyn Fn(&str) -> Option<String>>,
    content_only: bool,
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
                    style = styled(part, inherited, content_only)?;
                }
                component_mode(
                    part,
                    style,
                    out,
                    remaining,
                    depth + 1,
                    translate,
                    content_only,
                )?;
            }
            Ok(())
        }
        NbtValue::Compound(c) => {
            let style = styled(v, inherited, content_only)?;
            if let Some(NbtValue::String(s)) = c.get("text") {
                literal(s, style, out)?;
            } else if let (Some(lookup), Some(NbtValue::String(key))) =
                (translate, c.get("translate"))
            {
                let fallback = match c.get("fallback") {
                    Some(NbtValue::String(s)) => Some(s.as_ref()),
                    None => None,
                    _ => return Err(()),
                };
                let format = lookup(key)
                    .or_else(|| fallback.map(str::to_owned))
                    .unwrap_or_else(|| key.to_string());
                let args = match c.get("with") {
                    Some(NbtValue::List(args)) => args.as_slice(),
                    None => &[],
                    _ => return Err(()),
                };
                translated(
                    &format,
                    args,
                    style,
                    out,
                    remaining,
                    depth,
                    Translation {
                        lookup,
                        content_only,
                    },
                )?;
            } else {
                return Err(());
            }
            if let Some(extra) = c.get("extra") {
                let NbtValue::List(parts) = extra else {
                    return Err(());
                };
                for part in parts {
                    component_mode(
                        part,
                        style,
                        out,
                        remaining,
                        depth + 1,
                        translate,
                        content_only,
                    )?;
                }
            }
            Ok(())
        }
        _ => Err(()),
    }
}
#[derive(Clone, Copy)]
struct Translation<'a> {
    lookup: &'a dyn Fn(&str) -> Option<String>,
    content_only: bool,
}
fn translated(
    mut text: &str,
    args: &[NbtValue],
    style: Style,
    out: &mut String,
    remaining: &mut usize,
    depth: usize,
    translation: Translation<'_>,
) -> Result<(), ()> {
    let Translation {
        lookup,
        content_only,
    } = translation;
    let mut next = 0usize;
    while let Some(at) = text.find('%') {
        if *remaining == 0 {
            return Err(());
        }
        *remaining -= 1;
        literal(&text[..at], style, out)?;
        text = &text[at + 1..];
        if let Some(rest) = text.strip_prefix('%') {
            literal("%", style, out)?;
            text = rest;
            continue;
        }
        let digits = text.bytes().take_while(u8::is_ascii_digit).count();
        let index = if digits > 0 {
            let i = text[..digits]
                .parse::<usize>()
                .map_err(|_| ())?
                .checked_sub(1)
                .ok_or(())?;
            text = text[digits..].strip_prefix('$').ok_or(())?;
            i
        } else {
            let i = next;
            next = next.checked_add(1).ok_or(())?;
            i
        };
        text = text.strip_prefix('s').ok_or(())?;
        let arg = args.get(index).ok_or(())?;
        // NbtOps wraps heterogeneous primitive list entries in a single empty-key
        // compound. The pinned native mixed-argument codec vector verifies this.
        let arg = match arg {
            NbtValue::Compound(c) if c.iter().take(2).count() == 1 => c.get("").unwrap_or(arg),
            _ => arg,
        };
        match arg {
            NbtValue::Int(n) => literal(&n.to_string(), style, out)?,
            NbtValue::Long(n) => literal(&n.to_string(), style, out)?,
            value => component_mode(
                value,
                style,
                out,
                remaining,
                depth + 1,
                Some(lookup),
                content_only,
            )?,
        }
    }
    literal(text, style, out)
}
fn styled(v: &NbtValue, mut s: Style, content_only: bool) -> Result<Style, ()> {
    let NbtValue::Compound(c) = v else {
        return Ok(s);
    };
    let default_font = c.get("font").is_none_or(
        |font| matches!(font, NbtValue::String(name) if name.as_ref() == "minecraft:default"),
    );
    if !content_only
        && (c.boolean("underlined") == Some(true)
            || c.boolean("strikethrough") == Some(true)
            || !default_font
            || c.get("shadow_color").is_some())
    {
        return Err(());
    }
    if let Some(color) = c.get("color") {
        let NbtValue::String(name) = color else {
            return Err(());
        };
        let code = ui::BedrockColor::from_name(name)
            .and_then(|c| c.descriptor())
            .filter(|c| c.code.is_ascii_hexdigit())
            .map(|c| c.code);
        if code.is_none() && !content_only {
            return Err(());
        }
        if let Some(code) = code {
            s.color = Some(code);
        }
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
    fn native_default_font_is_not_an_unsupported_custom_font() {
        // Verified26.3 ComponentSerialization.CODEC emits this exact shape.
        let v = compound([
            (
                "text",
                NbtValue::String("You cannot send messages here.".into()),
            ),
            ("font", NbtValue::String("minecraft:default".into())),
        ]);
        assert_eq!(
            &*translated_component_text(&v, &|_| None).unwrap(),
            "§rYou cannot send messages here."
        );
    }
    #[test]
    fn partial_chat_presentation_retains_content_without_claiming_full_style_support() {
        for style in [
            ("underlined", NbtValue::Byte(1)),
            ("strikethrough", NbtValue::Byte(1)),
            ("color", NbtValue::String("#12ABEF".into())),
            ("font", NbtValue::String("fixture:custom".into())),
        ] {
            let v = compound([
                ("text", NbtValue::String("Actual server rejection".into())),
                style,
                (
                    "extra",
                    NbtValue::List(vec![NbtValue::String(" — retained".into())]),
                ),
            ]);
            assert!(translated_component_text(&v, &|_| None).is_err());
            assert_eq!(
                &*translated_component_content(&v, &|_| None).unwrap(),
                "§rActual server rejection§r — retained"
            );
        }
        let dynamic = compound([("selector", NbtValue::String("@a".into()))]);
        assert!(translated_component_content(&dynamic, &|_| None).is_err());
        assert!(
            translated_component_content(
                &NbtValue::String("X".repeat(ui::UiLimits::MAX_TEXT_BYTES).into()),
                &|_| None
            )
            .is_err()
        );
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
        component(&v, Style::default(), &mut out, &mut remaining, 0, None).unwrap();
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
                0,
                None
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
                0,
                None
            )
            .is_err()
        );
    }
    #[test]
    fn translations_preserve_ordered_arguments_unicode_and_literal_percent() {
        let value = compound([
            ("translate", NbtValue::String("fixture.message".into())),
            ("color", NbtValue::String("gold".into())),
            (
                "with",
                NbtValue::List(vec![
                    compound([("", NbtValue::String("夜".into()))]),
                    compound([("", NbtValue::Int(7))]),
                ]),
            ),
        ]);
        let out = translated_component_text(&value, &|k| {
            (k == "fixture.message").then(|| "%2$s: %1$s (100%%)".into())
        })
        .unwrap();
        let mut chars = out.chars();
        let mut plain = String::new();
        while let Some(c) = chars.next() {
            if c == '§' {
                chars.next();
            } else {
                plain.push(c);
            }
        }
        assert_eq!(plain, "7: 夜 (100%)");
        assert!(out.contains("§6"));
        let fallback = compound([
            ("translate", NbtValue::String("unknown".into())),
            ("fallback", NbtValue::String("Fallback".into())),
        ]);
        assert_eq!(
            translated_component_text(&fallback, &|_| None)
                .unwrap()
                .as_ref(),
            "§rFallback"
        );
        assert!(
            translated_component_text(&value, &|_| Some("%999999999999999999999999$s".into()))
                .is_err()
        );
    }
}
