use wc_metrics::{
    counter, enum_ordinalize::Ordinalize, BoolLabel, EnumLabel, OptionalBoolLabel,
    OptionalEnumLabel, OptionalStringLabel, StringLabel,
};

#[derive(Clone, Copy, Debug, Ordinalize)]
enum MyEnum {
    A,
    B,
}

impl wc_metrics::Enum for MyEnum {
    fn as_str(&self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }
}

pub fn counters(v: u64) {
    let s = "a";
    let b = true;
    let u = 42;
    let e = MyEnum::A;

    counter!("counter1").increment(v);

    counter!("counter2", EnumLabel<"e", MyEnum> => e).increment(v);

    counter!("counter3", BoolLabel<"b"> => b).increment(v);

    counter!("counter4", StringLabel<"s"> => s).increment(v);

    counter!("counter5", StringLabel<"s", u8> => &u).increment(v);

    counter!("counter6",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b
    )
    .increment(v);

    counter!("counter7", "st" => "1").increment(v);

    counter!("counter8", "st1" => "1", "st2" => "2").increment(v);

    counter!("counter9", StringLabel<"s", u8> => &u, "st" => "2").increment(v);

    counter!("counter10",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b,
        "st1" => "1",
        "st2" => "2"
    )
    .increment(v);

    counter!("counter11", "description11").increment(v);

    counter!("counter12", "description12", EnumLabel<"e", MyEnum> => e).increment(v);

    counter!("counter13", "description13", BoolLabel<"b"> => b).increment(v);

    counter!("counter14", "description14", StringLabel<"s"> => s).increment(v);

    counter!("counter15", "description15", StringLabel<"s", u8> => &u).increment(v);

    counter!("counter16", "description16",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b
    )
    .increment(v);

    counter!("counter17", "description17", "st" => "1").increment(v);

    counter!("counter18", "description18", "st1" => "1", "st2" => "2").increment(v);

    counter!("counter19", "description19", StringLabel<"s", u8> => &u, "st" => "2").increment(v);

    counter!("counter20", "description20",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b,
        OptionalEnumLabel<"oe", MyEnum> => Some(e),
        OptionalStringLabel<"os1"> => Some(s),
        OptionalStringLabel<"os2", u8> => Some(&u),
        OptionalBoolLabel<"ob"> => Some(b),
        "st1" => "1",
        "st2" => "2"
    )
    .increment(v);
}
