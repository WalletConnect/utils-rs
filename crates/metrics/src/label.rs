use {
    crate::{
        sealed::{Attrs, Execute},
        Metric,
    },
    arc_swap::ArcSwap,
    enum_ordinalize::Ordinalize,
    parking_lot::Mutex,
    smallvec::SmallVec,
    std::{borrow::Borrow, collections::HashMap, sync::Arc},
};

pub type DynamicLabels = SmallVec<[metrics::Label; 4]>;
pub type StaticLabels = &'static [(&'static str, &'static str)];

pub type Labeled<T, A> = WithLabel<A, T>;
pub type Labeled2<T, A, B> = WithLabel<A, WithLabel<B, T>>;
pub type Labeled3<T, A, B, C> = WithLabel<A, WithLabel<B, WithLabel<C, T>>>;
pub type Labeled4<T, A, B, C, D> = WithLabel<A, WithLabel<B, WithLabel<C, WithLabel<D, T>>>>;

/// Metric label defined as an `enum`.
///
/// The most efficient way to specify metric labels in runtime, metric lookups
/// using enum labels are as fast as array indexing. Prefer using this when all
/// of your possible label values are known at the compile time.
///
/// To implement this `trait` you also need to derive [`Ordinalize`] for your
/// `enum`.
/// SAFETY: DO NOT use custom discriminant values (eg. `enum MyEnum { MyVariant
/// = -1 }`), this will lead to either:
/// - `panic` in runtime (only for builds with `debug_assertions`)
/// - incorrect label resolution
pub trait EnumLabel: Copy + Ordinalize<VariantType = i8> {
    /// Name of the label.
    const NAME: &'static str;

    /// String representation of the label value.
    fn as_str(&self) -> &'static str;
}

pub trait DynamicLabel<M> {
    type MetricCollection;
}

pub trait ResolveLabels<L> {
    type Target;

    fn resolve_labels(&self, labels: L) -> &Self::Target;
}

/// Adds dynamically resolved (in runtime) label to `M`.
pub struct WithLabel<L: DynamicLabel<M>, M> {
    collection: L::MetricCollection,
}

impl<L, M> WithLabel<L, M>
where
    L: DynamicLabel<M>,
{
    /// Resolves a single dynamic label and finds the underlying metric.
    pub fn resolve_label<T>(&self, label: T) -> &M
    where
        Self: ResolveLabels<(T,), Target = M>,
    {
        self.resolve_labels((label,))
    }

    /// Resolves multilpe dynamic labels and finds the underlying metric.
    ///
    /// Expects a tuple of labels as `LS` argument.
    pub fn resolve_labels<LS>(&self, labels: LS) -> &<Self as ResolveLabels<LS>>::Target
    where
        Self: ResolveLabels<LS>,
    {
        ResolveLabels::resolve_labels(self, labels)
    }
}

impl<L, M> DynamicLabel<M> for L
where
    L: EnumLabel,
{
    // TODO: Switch to `[(L, M); L::VARIANT_COUNT]` once generic parameters are
    // allowed to be used in const expressions.
    type MetricCollection = Vec<(L, M)>;
}

impl<L, M> Metric for WithLabel<L, M>
where
    L: EnumLabel,
    M: Metric,
{
    fn register(attrs: &Attrs) -> Self {
        let metrics = L::VARIANTS.iter().map(|l| {
            let label = metrics::Label::from_static_parts(L::NAME, l.as_str());
            (*l, M::register(&attrs.with_label(label)))
        });

        Self {
            collection: metrics.collect(),
        }
    }
}

impl<L, M> ResolveLabels<(L,)> for WithLabel<L, M>
where
    L: EnumLabel,
    M: Metric,
{
    type Target = M;

    fn resolve_labels(&self, labels: (L,)) -> &M {
        let debug_panic = || {
            if cfg!(debug_assertions) {
                panic!("Invalid enum usage, custom discriminants must not be used")
            }
        };

        let idx = labels.0.ordinal();
        let mut idx = if idx < 0 {
            debug_panic();
            0
        } else {
            idx as usize
        };

        if idx >= self.collection.len() {
            debug_panic();
            idx = 0;
        };

        &self.collection[idx].1
    }
}

/// Label with the only possible values being `true` and `false`. A special
/// case of `EnumLabel` having the same peformance characteristics.
///
/// Due to the lack of `&'static str` const generics at the moment the label
/// name should be specified using the following hack:
///
/// ```
/// use metrics::{label_name, BoolLabel};
///
/// type MyLabel = BoolLabel<{ label_name("my_label") }>;
/// ```
pub struct BoolLabel<const NAME: LabelName>(bool);

impl<const NAME: LabelName> BoolLabel<NAME> {
    /// Creates a new [`BoolLabel`].
    pub fn new(b: bool) -> Self {
        Self(b)
    }
}

impl<const NAME: LabelName> From<BoolLabel<NAME>> for bool {
    fn from(label: BoolLabel<NAME>) -> Self {
        label.0
    }
}

impl<const NAME: LabelName, M> DynamicLabel<M> for BoolLabel<NAME> {
    type MetricCollection = (M, M);
}

impl<const NAME: LabelName, M> Metric for WithLabel<BoolLabel<NAME>, M>
where
    M: Metric,
{
    fn register(attrs: &Attrs) -> Self {
        let name = const { resolve_label_name::<NAME>() };

        let f = metrics::Label::from_static_parts(name, "false");
        let t = metrics::Label::from_static_parts(name, "true");

        Self {
            collection: (
                M::register(&attrs.with_label(f)),
                M::register(&attrs.with_label(t)),
            ),
        }
    }
}

impl<const NAME: LabelName, M> ResolveLabels<(BoolLabel<NAME>,)> for WithLabel<BoolLabel<NAME>, M>
where
    M: Metric,
{
    type Target = M;

    fn resolve_labels(&self, (label,): (BoolLabel<NAME>,)) -> &M {
        if label.0 {
            &self.collection.1
        } else {
            &self.collection.0
        }
    }
}

/// Label with values which are unknown at the complite time.
///
/// Metric lookups using these labels will be slower compared to [`EnumLabel`]s
/// and [`BoolLabel`]s. Prefer not to use these unless you really don't know
/// your label values beforehand.
///
/// Despite its name the label can accept any type that implements [`ToString`],
/// not just [`String`]s. The most frequent use-case (aside from the actual
/// strings) - numbers.
///
/// Due to the lack of `&'static str` const generics at the moment the label
/// name should be specified using the following hack:
///
/// ```
/// use metrics::{label_name, StringLabel};
///
/// type MyLabel = StringLabel<{ label_name("my_label") }>;
/// ```
pub struct StringLabel<const NAME: u128, T = String>(pub T);

impl<const NAME: LabelName, T> StringLabel<NAME, T> {
    /// Creates a new [`StringLabel`].
    ///
    /// Expects a [`Borrow`]ed version of your type as the owned version is not
    /// needed for the label resolution.
    pub fn new<U: ?Sized>(ref_: &U) -> StringLabel<NAME, &U>
    where
        T: Borrow<U>,
    {
        StringLabel(ref_)
    }

    /// Converts this [`StringLabel`] into the inner `T`.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<const NAME: LabelName, T> AsRef<T> for StringLabel<NAME, T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

pub struct StringCollection<T, M: 'static> {
    inner: ArcSwap<HashMap<T, &'static M>>,
    mutex: Mutex<()>,
    attrs: Attrs,
}

impl<const NAME: LabelName, T, M> DynamicLabel<M> for StringLabel<NAME, T>
where
    M: 'static,
{
    type MetricCollection = StringCollection<T, M>;
}

impl<const NAME: LabelName, T, M> Metric for WithLabel<StringLabel<NAME, T>, M>
where
    M: Metric + 'static,
{
    fn register(attrs: &Attrs) -> Self {
        Self {
            collection: StringCollection {
                inner: ArcSwap::new(Arc::new(HashMap::new())),
                mutex: Mutex::new(()),
                attrs: attrs.clone(),
            },
        }
    }
}

impl<const NAME: LabelName, T, U, M> ResolveLabels<(StringLabel<NAME, &U>,)>
    for WithLabel<StringLabel<NAME, T>, M>
where
    T: std::hash::Hash + Eq + Borrow<U> + ToString + Clone,
    U: std::hash::Hash + Eq + ToOwned<Owned = T> + ?Sized,
    M: Metric + 'static,
{
    type Target = M;

    fn resolve_labels(&self, (label,): (StringLabel<NAME, &U>,)) -> &M {
        let label = label.0;
        let col = &self.collection;

        if let Some(m) = col.inner.load().get(label) {
            return m;
        };

        let _guard = col.mutex.lock();

        let inner = col.inner.load();

        // In case if another thread has already initialized the metric while we were
        // waiting on the lock
        if let Some(m) = inner.get(label) {
            return m;
        };

        // Copy-on-write
        let m: &'static M = {
            // Make a deep copy of the `HashMap`.
            let mut inner_clone: HashMap<_, _> = (**inner).clone();

            let name = const { resolve_label_name::<NAME>() };
            let label_ = metrics::Label::new(name, label.to_owned().to_string());

            // Insert the new `Metric`.
            //
            // We are still holding the lock here and by doing so guaranteeing that
            // the static value is only being initialized once per key.
            //
            // Leaking is fine here as this collection can only be used inside
            // `static` variables and there should be limited amount of label
            // values defined in runtime.
            let m = Box::leak(Box::new(M::register(&col.attrs.with_label(label_))));
            inner_clone.insert(label.to_owned(), m);

            // Write the updated `HashMap` into `ArcSwap`.
            col.inner.store(Arc::new(inner_clone));
            m
        };

        m
    }
}

// TODO: macro to autogenerate these

impl<L, M, A, B> ResolveLabels<(A, B)> for WithLabel<L, M>
where
    L: DynamicLabel<M>,
    M: ResolveLabels<(B,)>,
    Self: ResolveLabels<(A,), Target = M>,
{
    type Target = M::Target;

    fn resolve_labels(&self, (a, b): (A, B)) -> &Self::Target {
        self.resolve_label(a).resolve_labels((b,))
    }
}

impl<L, M, A, B, C> ResolveLabels<(A, B, C)> for WithLabel<L, M>
where
    L: DynamicLabel<M>,
    M: ResolveLabels<(B, C)>,
    Self: ResolveLabels<(A,), Target = M>,
{
    type Target = M::Target;

    fn resolve_labels(&self, (a, b, c): (A, B, C)) -> &Self::Target {
        self.resolve_label(a).resolve_labels((b, c))
    }
}

impl<L, M, A, B, C, D> ResolveLabels<(A, B, C, D)> for WithLabel<L, M>
where
    L: DynamicLabel<M>,
    M: ResolveLabels<(B, C, D)>,
    Self: ResolveLabels<(A,), Target = M>,
{
    type Target = M::Target;

    fn resolve_labels(&self, (a, b, c, d): (A, B, C, D)) -> &Self::Target {
        self.resolve_label(a).resolve_labels((b, c, d))
    }
}

impl<L, M, Op, LS> Execute<Op, LS> for WithLabel<L, M>
where
    L: DynamicLabel<M>,
    Self: ResolveLabels<LS, Target: Execute<Op, ()>>,
{
    fn execute(&self, op: Op, labels: LS) {
        self.resolve_labels(labels).execute(op, ())
    }
}

/// `u128` representation of `&'static str` label name.
pub type LabelName = u128;

/// Converts a `&'static str` into a byte-wise equivalent `u128`.
///
/// Required to hack around the lack of const `&'static str` generics in stable
/// Rust.
pub const fn label_name(s: &'static str) -> LabelName {
    let bytes = s.as_bytes();

    assert!(
        bytes.len() <= 16,
        "`LabelName` should be no longer than 16 bytes"
    );

    // loops aren't supported in const fns
    const fn copy(idx: usize, src: &[u8], mut dst: [u8; 16]) -> [u8; 16] {
        if idx == src.len() {
            return dst;
        }

        dst[idx] = src[idx];
        copy(idx + 1, src, dst)
    }

    u128::from_be_bytes(copy(0, bytes, [0u8; 16]))
}

const fn resolve_label_name<const N: LabelName>() -> &'static str {
    let bytes = Const::<N>::BYTES;

    // Find the index of the first null byte
    const fn null_byte_idx(b: &[u8], idx: usize) -> usize {
        if idx == b.len() {
            return idx;
        }

        if b[idx] == 0 {
            return idx;
        }

        null_byte_idx(b, idx + 1)
    }

    // truncate null bytes
    let (bytes, _) = bytes.split_at(null_byte_idx(bytes, 0));

    match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => panic!("Invalid utf8"),
    }
}

trait ConstByteSlice {
    const BYTES: &'static [u8];
}

struct Const<const U: u128>;

impl<const U: u128> ConstByteSlice for Const<U> {
    const BYTES: &'static [u8] = &U.to_be_bytes();
}

#[test]
fn test_label_name() {
    const A: LabelName = label_name("test");
    let name = const { resolve_label_name::<A>() };
    assert_eq!(name, "test");
}
