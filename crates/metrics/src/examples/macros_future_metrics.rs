use wc_metrics::{
    enum_ordinalize::Ordinalize, future_metrics, BoolLabel, EnumLabel, FutureExt,
    OptionalBoolLabel, OptionalEnumLabel, OptionalStringLabel, StringLabel,
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

pub async fn future_metrics() {
    let s = "a";
    let b = true;
    let u = 42;
    let e = MyEnum::A;

    async {}
        .with_metrics(future_metrics!("future_metrics1"))
        .await;

    async {}
        .with_metrics(future_metrics!("future_metrics2", EnumLabel<"e", MyEnum> => e))
        .await;

    async {}
        .with_metrics(future_metrics!("future_metrics3", BoolLabel<"b"> => b))
        .await;

    async {}
        .with_metrics(future_metrics!("future_metrics4", StringLabel<"s"> => s))
        .await;

    async {}
        .with_metrics(future_metrics!("future_metrics5", StringLabel<"s", u8> => &u))
        .await;

    async {}
        .with_metrics(future_metrics!("future_metrics6",
            EnumLabel<"e", MyEnum> => e,
            StringLabel<"s1"> => s,
            StringLabel<"s2", u8> => &u,
            BoolLabel<"b"> => b
        ))
        .await;

    async {}
        .with_metrics(future_metrics!("future_metrics7", "st" => "1"))
        .await;

    async {}
        .with_metrics(future_metrics!("future_metrics8", "st1" => "1", "st2" => "2"))
        .await;

    async {}
        .with_metrics(future_metrics!("future_metrics9", StringLabel<"s", u8> => &u, "st" => "2"))
        .await;

    async {}
        .with_metrics(future_metrics!("future_metrics10",
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
        ))
        .await;
}
