//! Pinned MerchantOffers framing. Validation does not implement trading or authorize clicks.
use super::*;

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub container: i32,
    pub offers: usize,
    pub level: i32,
    pub experience: i32,
    pub show_progress: bool,
    pub can_restock: bool,
}
fn cost(r: &mut Reader<'_>) -> Result<(), Error> {
    let item = nonnegative(r)? as usize;
    target::ITEM_TYPES
        .get(item)
        .ok_or(Error::Invalid("unknown native merchant item"))?;
    r.var()?; // ItemCost's stream count is signed, unlike its positive-int disk codec.
    // DataComponentExactPredicate is a list of typed components, not an ItemStack patch.
    for _ in 0..r.count(r.0.len())? {
        let spec = component(r)?;
        components::read(spec, r, 0)?;
    }
    Ok(())
}
pub(super) fn read(r: &mut Reader<'_>) -> Result<Summary, Error> {
    let container = nonnegative(r)?;
    let offers = r.count(r.0.len())?;
    for _ in 0..offers {
        cost(r)?;
        stack(r)?;
        if r.bool()? {
            cost(r)?;
        }
        r.bool()?; // Out of stock.
        for _ in 0..4 {
            r.i32()?;
        } // Uses, maximum uses, XP, special price.
        r.f32()?; // Price multiplier.
        r.i32()?; // Demand.
    }
    Ok(Summary {
        container,
        offers,
        level: r.var()?,
        experience: r.var()?,
        show_progress: r.bool()?,
        can_restock: r.bool()?,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_cost_predicates_secondary_cost_and_footer_remain_aligned() {
        let item = target::ITEM_TYPES
            .iter()
            .position(|n| *n == "minecraft:emerald")
            .unwrap() as i32;
        let mut b = vec![7, 1];
        crate::codec::write_varint(item, &mut b);
        b.extend([2, 1]);
        let component = target::ITEM_COMPONENTS
            .iter()
            .position(|s| s.name == "minecraft:max_stack_size")
            .unwrap() as i32;
        crate::codec::write_varint(component, &mut b);
        b.push(16); // Cost item, count, exact-predicate list.
        b.push(3);
        crate::codec::write_varint(item, &mut b);
        b.extend([0, 0]); // Result count/id/empty patch.
        b.push(1);
        crate::codec::write_varint(item, &mut b);
        b.extend([1, 0]); // Optional second cost.
        b.push(0);
        for n in [2i32, 20, 5, -3] {
            b.extend(n.to_be_bytes());
        }
        b.extend(0.05f32.to_be_bytes());
        b.extend(4i32.to_be_bytes());
        b.extend([2, 17, 1, 0]);
        let hex = include_str!("../../../data/merchant-offers-fixture.hex").trim();
        let native = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(b, native, "pinned native codec bytes");
        let mut r = Reader(&b);
        let value = read(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!(
            (value.container, value.offers, value.level, value.experience),
            (7, 1, 2, 17)
        );
        assert!(value.show_progress);
        assert!(!value.can_restock);
        for end in 0..b.len() {
            assert!(read(&mut Reader(&b[..end])).is_err(), "truncation {end}");
        }
    }
    #[test]
    fn empty_catalog_is_complete_but_does_not_imply_trading_support() {
        let mut r = Reader(&[1, 0, 1, 0, 0, 1]);
        let s = read(&mut r).unwrap();
        r.finish().unwrap();
        assert_eq!((s.container, s.offers), (1, 0));
        assert!(s.can_restock);
        assert!(read(&mut Reader(&[1, 127])).is_err());
    }
}
