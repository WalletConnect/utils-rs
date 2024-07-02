/// Similar to [`metrics::counter`](crate::backend::counter), but expects
/// dynamic labels to be type-annotated and provides an option to specify
/// metric descriptions.
///
/// Uses the machinery of this crate to create appropriately-typed `static`
/// metric and to resolve dynamic labels.
///
/// Using this macro with the same arguments multilpe times is not recommended
/// as each time it creates a separate `static` variable.
/// If your metric needs to be modified from multiple places either store it
/// inside your own types, or use the vanilla machinery of this crate to define
/// your own `static` metric and use it instead.
///
/// Usage:
/// ```
/// use wc_metrics::{
///     counter,
///     enum_ordinalize::Ordinalize,
///     BoolLabel,
///     EnumLabel,
///     OptionalBoolLabel,
///     OptionalEnumLabel,
///     OptionalStringLabel,
///     StringLabel,
/// };
///
/// #[derive(Clone, Copy, Debug, Ordinalize)]
/// enum MyEnum {
///     A,
///     B,
/// }
///
/// impl wc_metrics::Enum for MyEnum {
///     fn as_str(&self) -> &'static str {
///         match self {
///             Self::A => "a",
///             Self::B => "b",
///         }
///     }
/// }
///
/// let s = "a";
/// let b = true;
/// let u = 42;
/// let e = MyEnum::A;
///
/// counter!("name").increment(1);
///
/// counter!("name", EnumLabel<"a", MyEnum> => e).increment(1);
///
/// counter!("name", BoolLabel<"a"> => b).increment(1);
///
/// counter!("name", StringLabel<"a"> => s).increment(1);
///
/// counter!("name", StringLabel<"a", u8> => &u).increment(1);
///
/// counter!("name",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b
/// )
/// .increment(1);
///
/// counter!("name", "a" => "1").increment(1);
///
/// counter!("name", "a" => "1", "b" => "2").increment(1);
///
/// counter!("name", StringLabel<"a", u8> => &u, "b" => "2").increment(1);
///
/// counter!("name",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b,
///     "e" => "1",
///     "f" => "2"
/// )
/// .increment(1);
///
/// counter!("name", "description").increment(1);
//
/// counter!("name", "description", EnumLabel<"a", MyEnum> => e).increment(1);
///
/// counter!("name", "description", BoolLabel<"a"> => b).increment(1);
///
/// counter!("name", "description", StringLabel<"a"> => s).increment(1);
///
/// counter!("name", "description", StringLabel<"a", u8> => &u).increment(1);
///
/// counter!("name", "description",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b
/// )
/// .increment(1);
///
/// counter!("name", "description", "a" => "1").increment(1);
///
/// counter!("name", "description", "a" => "1", "b" => "2").increment(1);
///
/// counter!("name", "description", StringLabel<"a", u8> => &u, "b" =>
/// "2").increment(1);
///
/// counter!("name", "description",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b,
///     OptionalEnumLabel<"e", MyEnum> => Some(e),
///     OptionalStringLabel<"f"> => Some(s),
///     OptionalStringLabel<"g", u8> => Some(&u),
///     OptionalBoolLabel<"h"> => Some(b),
///     "i" => "1",
///     "j" => "2"
/// )
/// .increment(1);
/// ```
#[macro_export]
macro_rules! counter {
    ($($tail:tt)*) => {
        $crate::metric!($crate::backend::Counter, $($tail)*)
    };
}

/// Similar to [`metrics::gauge`](crate::backend::gauge), but expects
/// dynamic labels to be type-annotated and provides an option to specify
/// metric descriptions.
///
/// Uses the machinery of this crate to create appropriately-typed `static`
/// metric and to resolve dynamic labels.
///
/// Using this macro with the same arguments multilpe times is not recommended
/// as each time it creates a separate `static` variable.
/// If your metric needs to be modified from multiple places either store it
/// inside your own types, or use the vanilla machinery of this crate to define
/// your own `static` metric and use it instead.
///
/// Usage:
/// ```
/// use wc_metrics::{
///     gauge,
///     enum_ordinalize::Ordinalize,
///     BoolLabel,
///     EnumLabel,
///     OptionalBoolLabel,
///     OptionalEnumLabel,
///     OptionalStringLabel,
///     StringLabel,
/// };
///
/// #[derive(Clone, Copy, Debug, Ordinalize)]
/// enum MyEnum {
///     A,
///     B,
/// }
///
/// impl wc_metrics::Enum for MyEnum {
///     fn as_str(&self) -> &'static str {
///         match self {
///             Self::A => "a",
///             Self::B => "b",
///         }
///     }
/// }
///
/// let s = "a";
/// let b = true;
/// let u = 42;
/// let e = MyEnum::A;
///
/// gauge!("name").set(1);
///
/// gauge!("name", EnumLabel<"a", MyEnum> => e).set(1);
///
/// gauge!("name", BoolLabel<"a"> => b).set(1);
///
/// gauge!("name", StringLabel<"a"> => s).set(1);
///
/// gauge!("name", StringLabel<"a", u8> => &u).set(1);
///
/// gauge!("name",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b
/// )
/// .set(1);
///
/// gauge!("name", "a" => "1").set(1);
///
/// gauge!("name", "a" => "1", "b" => "2").set(1);
///
/// gauge!("name", StringLabel<"a", u8> => &u, "b" => "2").set(1);
///
/// gauge!("name",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b,
///     "e" => "1",
///     "f" => "2"
/// )
/// .set(1);
///
/// gauge!("name", "description").set(1);
//
/// gauge!("name", "description", EnumLabel<"a", MyEnum> => e).set(1);
///
/// gauge!("name", "description", BoolLabel<"a"> => b).set(1);
///
/// gauge!("name", "description", StringLabel<"a"> => s).set(1);
///
/// gauge!("name", "description", StringLabel<"a", u8> => &u).set(1);
///
/// gauge!("name", "description",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b
/// )
/// .set(1);
///
/// gauge!("name", "description", "a" => "1").set(1);
///
/// gauge!("name", "description", "a" => "1", "b" => "2").set(1);
///
/// gauge!("name", "description", StringLabel<"a", u8> => &u, "b" =>
/// "2").set(1);
///
/// gauge!("name", "description",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b,
///     OptionalEnumLabel<"e", MyEnum> => Some(e),
///     OptionalStringLabel<"f"> => Some(s),
///     OptionalStringLabel<"g", u8> => Some(&u),
///     OptionalBoolLabel<"h"> => Some(b),
///     "i" => "1",
///     "j" => "2"
/// )
/// .set(1);
/// ```
#[macro_export]
macro_rules! gauge {
    ($($tail:tt)*) => {
        $crate::metric!($crate::backend::Gauge, $($tail)*)
    };
}

/// Similar to [`metrics::histogram`](crate::backend::histogram), but expects
/// dynamic labels to be type-annotated and provides an option to specify
/// metric descriptions.
///
/// Uses the machinery of this crate to create appropriately-typed `static`
/// metric and to resolve dynamic labels.
///
/// Using this macro with the same arguments multilpe times is not recommended
/// as each time it creates a separate `static` variable.
/// If your metric needs to be modified from multiple places either store it
/// inside your own types, or use the vanilla machinery of this crate to define
/// your own `static` metric and use it instead.
///
/// Usage:
/// ```
/// use wc_metrics::{
///     histogram,
///     enum_ordinalize::Ordinalize,
///     BoolLabel,
///     EnumLabel,
///     OptionalBoolLabel,
///     OptionalEnumLabel,
///     OptionalStringLabel,
///     StringLabel,
/// };
///
/// #[derive(Clone, Copy, Debug, Ordinalize)]
/// enum MyEnum {
///     A,
///     B,
/// }
///
/// impl wc_metrics::Enum for MyEnum {
///     fn as_str(&self) -> &'static str {
///         match self {
///             Self::A => "a",
///             Self::B => "b",
///         }
///     }
/// }
///
/// let s = "a";
/// let b = true;
/// let u = 42;
/// let e = MyEnum::A;
///
/// histogram!("name").record(1);
///
/// histogram!("name", EnumLabel<"a", MyEnum> => e).record(1);
///
/// histogram!("name", BoolLabel<"a"> => b).record(1);
///
/// histogram!("name", StringLabel<"a"> => s).record(1);
///
/// histogram!("name", StringLabel<"a", u8> => &u).record(1);
///
/// histogram!("name",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b
/// )
/// .record(1);
///
/// histogram!("name", "a" => "1").record(1);
///
/// histogram!("name", "a" => "1", "b" => "2").record(1);
///
/// histogram!("name", StringLabel<"a", u8> => &u, "b" => "2").record(1);
///
/// histogram!("name",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b,
///     "e" => "1",
///     "f" => "2"
/// )
/// .record(1);
///
/// histogram!("name", "description").record(1);
//
/// histogram!("name", "description", EnumLabel<"a", MyEnum> => e).record(1);
///
/// histogram!("name", "description", BoolLabel<"a"> => b).record(1);
///
/// histogram!("name", "description", StringLabel<"a"> => s).record(1);
///
/// histogram!("name", "description", StringLabel<"a", u8> => &u).record(1);
///
/// histogram!("name", "description",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b
/// )
/// .record(1);
///
/// histogram!("name", "description", "a" => "1").record(1);
///
/// histogram!("name", "description", "a" => "1", "b" => "2").record(1);
///
/// histogram!("name", "description", StringLabel<"a", u8> => &u, "b" =>
/// "2").record(1);
///
/// histogram!("name", "description",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b,
///     OptionalEnumLabel<"e", MyEnum> => Some(e),
///     OptionalStringLabel<"f"> => Some(s),
///     OptionalStringLabel<"g", u8> => Some(&u),
///     OptionalBoolLabel<"h"> => Some(b),
///     "i" => "1",
///     "j" => "2"
/// )
/// .record(1);
/// ```
#[macro_export]
macro_rules! histogram {
    ($($tail:tt)*) => {
        $crate::metric!($crate::backend::Histogram, $($tail)*)
    };
}

/// Similar to [`counter`], [`gauge`] and [`histogram`], but operates with
/// [`FutureMetrics`](crate::FutureMetrics) instead.
///
/// Usage:
/// ```
/// use std::future::Future;
/// use wc_metrics::{
///     future_metrics,
///     enum_ordinalize::Ordinalize,
///     BoolLabel,
///     EnumLabel,
///     OptionalBoolLabel,
///     OptionalEnumLabel,
///     OptionalStringLabel,
///     StringLabel,
///     FutureExt,
///     FutureMetrics,
/// };
///
/// #[derive(Clone, Copy, Debug, Ordinalize)]
/// enum MyEnum {
///     A,
///     B,
/// }
///
/// impl wc_metrics::Enum for MyEnum {
///     fn as_str(&self) -> &'static str {
///         match self {
///             Self::A => "a",
///             Self::B => "b",
///         }
///     }
/// }
///
/// let s = "a";
/// let b = true;
/// let u = 42;
/// let e = MyEnum::A;
///
/// fn future(metrics: &'static FutureMetrics) -> impl Future {
///    async {}.with_metrics(metrics)
/// }
///
/// fn spawn(f: impl Future) {}
///
/// spawn(async {}.with_metrics(future_metrics!("name")));
///
/// spawn(async {}.with_metrics(future_metrics!("name", EnumLabel<"a", MyEnum> => e)));
///
/// spawn(async {}.with_metrics(future_metrics!("name", BoolLabel<"a"> => b)));
///
/// spawn(async {}.with_metrics(future_metrics!("name", StringLabel<"a"> => s)));
///
/// spawn(async {}.with_metrics(future_metrics!("name", StringLabel<"a", u8> => &u)));
///
/// spawn(async {}.with_metrics(future_metrics!("name",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b
/// )));
///
/// spawn(async {}.with_metrics(future_metrics!("name", "a" => "1")));
///
/// spawn(async {}.with_metrics(future_metrics!("name", "a" => "1", "b" => "2")));
///
/// spawn(async {}.with_metrics(future_metrics!("name", StringLabel<"a", u8> => &u, "b" => "2")));
///
/// spawn(async {}.with_metrics(future_metrics!("name",
///     EnumLabel<"a", MyEnum> => e,
///     StringLabel<"b"> => s,
///     StringLabel<"c", u8> => &u,
///     BoolLabel<"d"> => b,
///     OptionalEnumLabel<"e", MyEnum> => Some(e),
///     OptionalStringLabel<"f"> => Some(s),
///     OptionalStringLabel<"g", u8> => Some(&u),
///     OptionalBoolLabel<"h"> => Some(b),
///     "i" => "1",
///     "j" => "2"
/// )));
/// ```
#[cfg(feature = "future")]
#[macro_export]
macro_rules! future_metrics {
    ($($tail:tt)*) => {
        $crate::metric!($crate::FutureMetrics, $($tail)*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! metric {
    ( $type:ty, $name:literal) => {
        {
            static METRIC: $crate::Lazy<$type> = $crate::new($name);
            &METRIC
        }
    };

    ( $type:ty, $name:literal, $description:literal) => {
        {
            static METRIC: $crate::Lazy<$type> = $crate::builder($name)
                .with_description($description)
                .build();
            &METRIC
        }
    };

    ( $type:ty, $name:literal, $description:literal, $($tail:tt)*) => {
        {
            static METRIC: $crate::Lazy<$crate::metric_type!($type, $($tail)*)> = $crate::builder($name)
                .with_description($description)
                .with_static_labels($crate::static_labels!($($tail)*))
                .build();

            let m = &METRIC;
            $crate::resolve_labels!(m, $($tail)*);
            m
        }
    };
    ( $type:ty, $name:literal, $($tail:tt)*) => {
        {
            static METRIC: $crate::Lazy<$crate::metric_type!($type, $($tail)*)> = $crate::builder($name)
                .with_static_labels($crate::static_labels!($($tail)*))
                .build();

            let m = &METRIC;
            $crate::resolve_labels!(m, $($tail)*);
            m
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! metric_type {
    ( $type:ty, $( $_:literal => $__:literal ),+ )=> {
        $type
    };
    ( $type:ty, $label_type_name:ident<$label_name:literal$(,$inner_ty:ty)?> => $label_value:expr, $( $_:literal => $__:literal ),+ )=> {
        $crate::WithLabel<$label_type_name<{ $crate::label_name($label_name) }$(,$inner_ty)?>, $type>
    };
    ( $type:ty, $label_type_name:ident<$label_name:literal$(,$inner_ty:ty)?> => $label_value:expr, $($tail:tt)*) => {
        $crate::WithLabel<$label_type_name<{ $crate::label_name($label_name) }$(,$inner_ty)?>, $crate::metric_type!($type, $($tail)*)>
    };
    ( $type:ty, $label_type_name:ident<$label_name:literal$(,$inner_ty:ty)?> => $label_value:expr )=> {
        $crate::WithLabel<$label_type_name<{ $crate::label_name($label_name) }$(,$inner_ty)?>, $type>
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! static_labels {
    ( $label_type_name:ident<$label_name:literal$(,$inner_ty:ty)?> => $label_value:expr) => {
        &[]
    };
    ( $label_type_name:ident<$label_name:literal$(,$inner_ty:ty)?> => $label_value:expr, $($tail:tt)*) => {
        $crate::static_labels!($($tail)*)
    };
    ( $( $label_name:literal => $label_value:literal ),+ ) => {
        &[$(($label_name, $label_value),)*]
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! resolve_labels {
    ( $var:ident, $label_type_name:ident<$label_name:literal$(,$inner_ty:ty)?> => $label_value:expr, $($tail:tt)*) => {
        let $var = $var.resolve_label($label_type_name::<{ $crate::label_name($label_name) }$(,$inner_ty)?>::new($label_value));
        $crate::resolve_labels!($var, $($tail)*)
    };
    ( $var:ident, $label_type_name:ident<$label_name:literal$(,$inner_ty:ty)?> => $label_value:expr )=> {
        let $var = $var.resolve_label($label_type_name::<{ $crate::label_name($label_name) }$(,$inner_ty)?>::new($label_value));
    };
    ( $var:ident, $( $label_name:literal => $label_value:literal ),+ ) => {};
}
