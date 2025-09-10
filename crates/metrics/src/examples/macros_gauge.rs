use wc_metrics::{
    enum_ordinalize::Ordinalize, gauge, BoolLabel, EnumLabel, OptionalBoolLabel, OptionalEnumLabel,
    OptionalStringLabel, StringLabel,
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

pub fn gauges(v: f64) {
    let s = "a";
    let b = true;
    let u = 42;
    let e = MyEnum::A;

    gauge!("gauge1").set(v);

    gauge!("gauge2", EnumLabel<"e", MyEnum> => e).set(v);

    gauge!("gauge3", BoolLabel<"b"> => b).set(v);

    gauge!("gauge4", StringLabel<"s"> => s).set(v);

    gauge!("gauge5", StringLabel<"s", u8> => &u).set(v);

    gauge!("gauge6",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b
    )
    .set(v);

    gauge!("gauge7", "st" => "1").set(v);

    gauge!("gauge8", "st1" => "1", "st2" => "2").set(v);

    gauge!("gauge9", StringLabel<"s", u8> => &u, "st" => "2").set(v);

    gauge!("gauge10",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b,
        "st1" => "1",
        "st2" => "2"
    )
    .set(v);

    gauge!("gauge11", "description11").set(v);

    gauge!("gauge12", "description12", EnumLabel<"e", MyEnum> => e).set(v);

    gauge!("gauge13", "description13", BoolLabel<"b"> => b).set(v);

    gauge!("gauge14", "description14", StringLabel<"s"> => s).set(v);

    gauge!("gauge15", "description15", StringLabel<"s", u8> => &u).set(v);

    gauge!("gauge16", "description16",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b
    )
    .set(v);

    gauge!("gauge17", "description17", "st" => "1").set(v);

    gauge!("gauge18", "description18", "st1" => "1", "st2" => "2").set(v);

    gauge!("gauge19", "description19", StringLabel<"s", u8> => &u, "st" => "2").set(v);

    gauge!("gauge20", "description20",
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
    .set(v);
}
