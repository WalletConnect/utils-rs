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
#[doc = include_str!("examples/macros_counter.rs")]
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
#[doc = include_str!("examples/macros_gauge.rs")]
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
#[doc = include_str!("examples/macros_histogram.rs")]
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
#[doc = include_str!("examples/macros_future_metrics.rs")]
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
