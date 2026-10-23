//! Native NBT admission shared by terrain and player-list domain facts.
use java_protocol::nbt::Value;
pub(super) fn convert(value: Value) -> world::NbtValue {
    use world::NbtValue as V;
    match value {
        Value::Byte(n) => V::Byte(n),
        Value::Short(n) => V::Short(n),
        Value::Int(n) => V::Int(n),
        Value::Long(n) => V::Long(n),
        Value::Float(n) => V::Float(n),
        Value::Double(n) => V::Double(n),
        Value::Bytes(n) => V::ByteArray(n),
        Value::String(n) => V::String(n.into()),
        Value::Ints(n) => V::IntArray(n),
        Value::Longs(n) => V::LongArray(n),
        Value::List(n) => V::List(n.into_iter().map(convert).collect()),
        Value::Compound(n) => {
            let mut c = world::NbtCompound::default();
            for (k, v) in n {
                c.insert(k, convert(v));
            }
            V::Compound(c)
        }
    }
}
