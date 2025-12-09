use wc_metrics::{
    enum_ordinalize::Ordinalize,
    histogram,
    BoolLabel,
    EnumLabel,
    OptionalBoolLabel,
    OptionalEnumLabel,
    OptionalStringLabel,
    StringLabel,
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

pub fn histograms(v: f64) {
    let s = "a";
    let b = true;
    let u = 42;
    let e = MyEnum::A;

    histogram!("histogram1").record(v);

    histogram!("histogram2", EnumLabel<"e", MyEnum> => e).record(v);

    histogram!("histogram3", BoolLabel<"b"> => b).record(v);

    histogram!("histogram4", StringLabel<"s"> => s).record(v);

    histogram!("histogram5", StringLabel<"s", u8> => &u).record(v);

    histogram!("histogram6",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b
    )
    .record(v);

    histogram!("histogram7", "st" => "1").record(v);

    histogram!("histogram8", "st1" => "1", "st2" => "2").record(v);

    histogram!("histogram9", StringLabel<"s", u8> => &u, "st" => "2").record(v);

    histogram!("histogram10",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b,
        "st1" => "1",
        "st2" => "2"
    )
    .record(v);

    histogram!("histogram11", "description11").record(v);

    histogram!("histogram12", "description12", EnumLabel<"e", MyEnum> => e).record(v);

    histogram!("histogram13", "description13", BoolLabel<"b"> => b).record(v);

    histogram!("histogram14", "description14", StringLabel<"s"> => s).record(v);

    histogram!("histogram15", "description15", StringLabel<"s", u8> => &u).record(v);

    histogram!("histogram16", "description16",
        EnumLabel<"e", MyEnum> => e,
        StringLabel<"s1"> => s,
        StringLabel<"s2", u8> => &u,
        BoolLabel<"b"> => b
    )
    .record(v);

    histogram!("histogram17", "description17", "st" => "1").record(v);

    histogram!("histogram18", "description18", "st1" => "1", "st2" => "2").record(v);

    histogram!("histogram19", "description19", StringLabel<"s", u8> => &u, "st" => "2").record(v);

    histogram!("histogram20", "description20",
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
    .record(v);
}
