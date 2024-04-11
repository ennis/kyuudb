//! DBSP circuits test
//!

use im::OrdMap;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, Bound};
use std::fmt;
use std::hash::Hash;
use std::ops::RangeBounds;
use std::process::Output;
use std::rc::Rc;

pub trait Idx: Copy + Ord + Hash + fmt::Debug + Default {
    const MIN: Self;
    const MAX: Self;

    fn to_u32(self) -> u32;
    fn from_u32(id: u32) -> Self;
    fn dummy() -> Self;
    fn next(self) -> Self;
}

fn range_helper<A: Idx, B: Idx>(a: impl RangeBounds<A>) -> (Bound<(A, B)>, Bound<(A, B)>) {
    let start = match a.start_bound() {
        Bound::Included(x) => Bound::Included((*x, B::MIN)),
        Bound::Excluded(x) => Bound::Excluded((*x, B::MAX)),
        Bound::Unbounded => Bound::Unbounded,
    };
    let end = match a.end_bound() {
        Bound::Included(x) => Bound::Included((*x, B::MAX)),
        Bound::Excluded(x) => Bound::Excluded((*x, B::MIN)),
        Bound::Unbounded => Bound::Unbounded,
    };
    (start, end)
}

/// A `Delta` represents a change to a table.
///
/// It is a key-value pair with a multiplicity (-1 when an element is removed, +1 when it is added).
struct Delta<K, V> {
    key: K,
    value: V,
    multiplicity: i32,
}

/// A value-multiplicity pair.
#[derive(Clone,Debug)]
struct ZVal<V> {
    value: V,
    multiplicity: i32,
}

trait Data: Clone + 'static {}

trait UnaryOp<I, O> {
    fn eval(&mut self, input: &I, output: &mut Vec<O>);
}

trait BinaryOp<I1, I2, O> {
    fn eval(&mut self, input1: &I1, input2: &I2, output: &mut Vec<O>);
}

type ZMap<K, V> = OrdMap<K, ZVal<V>>;

/// Integration operator.
///
/// Integrates a sequence of `Delta<K,V>` into a `Table<K,V>`.
struct Integrate<K, V> {
    state: ZMap<K, V>,
}

impl<K, V> Default for Integrate<K, V> {
    fn default() -> Self {
        Self {
            state: ZMap::new(),
        }
    }
}

impl<K, V> UnaryOp<Delta<K, V>, ZMap<K, V>> for Integrate<K, V>
where
    K: Copy+Ord+Hash,
    V: Clone,
{
    fn eval(&mut self, input: &Delta<K, V>, output: &mut Vec<ZMap<K, V>>) {
        self.state
            .entry(input.key)
            .and_modify(|v| {
                v.multiplicity += input.multiplicity;
            })
            .or_insert(ZVal {
                value: input.value.clone(),
                multiplicity: input.multiplicity,
            });
        output.push(self.state.clone());
    }
}

/*
/// Indexing operator.
///
/// Integrates a sequence of `Delta<K,V>` into a `ZMap<(IK,K),V>`, with a function `F(K,V) -> IK` to extract (or compute) the index key from the data.
struct Index<K, V, IK, F> {
    table: ZMap<(IK, K), V>,
    index_fn: F,
}

impl<K, V, IK, F> UnaryOp<Delta<K, V>, ZMap<(IK, K), V>> for Index<K, V, IK, F>
where
    F: Fn(&K, &V) -> IK,
{
    fn eval(&mut self, input: &Delta<K, V>) -> ZMap<(IK, K), V> {
        self.table.clone()
    }
}*/

struct FKJoinIndexed<F> {
    fk_fn: F,
}

impl<F> FKJoinIndexed<F> {
    fn new(fk_fn: F) -> Self {
        Self { fk_fn }
    }
}

impl<KA, VA, KB, VB, F> BinaryOp<ZMap<KA, VA>, Delta<KB, VB>, Delta<(KA, KB), (VA, VB)>>
    for FKJoinIndexed<F>
where
    KA: Idx,
    KB: Idx,
    VA: Clone,
    VB: Clone,
    F: Fn(&KB, &VB) -> KA,
{
    fn eval(
        &mut self,
        input1: &ZMap<KA, VA>,
        input2: &Delta<KB, VB>,
        output: &mut Vec<Delta<(KA, KB), (VA, VB)>>,
    ) {
        //input1.clone()
        let kb = input2.key;
        let ka = (self.fk_fn)(&kb, &input2.value);
        if let Some(va) = input1.get(&ka) {
            output.push(Delta {
                key: (ka, kb),
                value: (va.value.clone(), input2.value.clone()),
                multiplicity: va.multiplicity * input2.multiplicity,
            });
        }
    }
}

impl<KA, VA, KB, VB, F> BinaryOp<Delta<KA, VA>, ZMap<(KA, KB), VB>, Delta<(KA, KB), (VA, VB)>>
    for FKJoinIndexed<F>
where
    KA: Idx,
    KB: Idx,
    VA: Clone,
    VB: Clone,
    F: Fn(&KB, &VB) -> KA,
{
    fn eval(
        &mut self,
        input1: &Delta<KA, VA>,
        input2: &ZMap<(KA, KB), VB>,
        output: &mut Vec<Delta<(KA, KB), (VA, VB)>>,
    ) {
        let ka = input1.key;
        for ((ka,kb), vb) in input2.range(range_helper(ka..=ka)) {
            output.push(Delta {
                key: (*ka, *kb),
                value: (input1.value.clone(), vb.value.clone()),
                multiplicity: input1.multiplicity * vb.multiplicity,
            });
        }
    }
}

#[cfg(test)]
mod test {
    use crate::circuit::{BinaryOp, Delta, FKJoinIndexed, Integrate, UnaryOp, ZMap, ZVal};
    use super::Idx;

    macro_rules! make_id {
        ($name:ident) => {
            #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
            #[repr(transparent)]
            pub struct $name(pub(crate) std::num::NonZeroU32);

            impl Idx for $name {
                const MIN: $name = $name(std::num::NonZeroU32::MIN);
                const MAX: $name = $name(std::num::NonZeroU32::MAX);

                fn to_u32(self) -> u32 {
                    self.0.get() - 1
                }

                fn from_u32(id: u32) -> $name {
                    $name(unsafe { std::num::NonZeroU32::new_unchecked(id + 1) })
                }

                fn dummy() -> $name {
                    $name(unsafe { std::num::NonZeroU32::new_unchecked(u32::MAX) })
                }

                fn next(self) -> $name {
                    $name::from_u32(self.to_u32() + 1)
                }
            }

            impl Default for $name {
                fn default() -> Self {
                    $name::from_u32(0)
                }
            }
        };
    }

    make_id!(AlbumId);
    make_id!(TrackId);
    make_id!(PlaylistId);
    make_id!(ArtistId);

    #[derive(Clone,Debug,Default)]
    struct Album {
        id: AlbumId,
        name: String,
    }

    #[derive(Clone, Debug,Default)]
    struct Track {
        id: TrackId,
        name: String,
        album: AlbumId,
    }

    #[test]
    fn test_join() {
        let mut album_counter = AlbumId::default();
        let mut track_counter = TrackId::default();

        let mut albums : Vec<Delta<AlbumId, Album>> = vec![];
        let mut tracks : Vec<Delta<TrackId, Track>> = vec![];

        let mut make_album = |name: &str| {
            let id = album_counter;
            let album = Album {
                id,
                name: name.to_string(),
            };
            albums.push(Delta {
                key: id,
                value: album,
                multiplicity: 1,
            });
            album_counter = album_counter.next();
            id
        };

        let mut make_track = |name: &str, album: AlbumId| {
            let id = track_counter;
            let track = Track {
                id,
                name: name.to_string(),
                album,
            };
            tracks.push(Delta {
                key: id,
                value: track,
                multiplicity: 1,
            });
            track_counter = track_counter.next();
            id
        };

        /*
            Album: シンクロ0
            Tracks:
            - デンデラパーティーナイト
            - 月詠-彦星-
            - 月面コールスター
            - ふたりあわせ
            - マヨナカトリップ
            - オブザーバーズセオリー
            - インスタントブルー
            - 一度は詣れよ善光寺
            - SF
            - ロマンモンスター
            - 時間よ止まれ

            Album: 絶倫パトロール
            (新)ロマンティック
            スーパースター
            神楽0
            絶倫パトロール
            良くも悪くも。
            愛の消費期限
            神楽0(off vocal)
            絶倫パトロール(off vocal)
        */

        let shinkuro = make_album("シンクロ0");
        let zetsurin = make_album("絶倫パトロール");
        let shinkuro_dendera = make_track("デンデラパーティーナイト", shinkuro);
        let shinkuro_tsukuyomi = make_track("月詠-彦星-", shinkuro);
        let shinkuro_getsumen = make_track("月面コールスター", shinkuro);
        let shinkuro_futari = make_track("ふたりあわせ", shinkuro);
        let shinkuro_mayonaka = make_track("マヨナカトリップ", shinkuro);
        let shinkuro_observers = make_track("オブザーバーズセオリー", shinkuro);
        let shinkuro_instant = make_track("インスタントブルー", shinkuro);
        let shinkuro_ichido = make_track("一度は詣れよ善光寺", shinkuro);
        let shinkuro_sf = make_track("SF", shinkuro);
        let shinkuro_roman = make_track("ロマンモンスター", shinkuro);
        let shinkuro_jikan = make_track("時間よ止まれ", shinkuro);

        let zetsurin_romantic = make_track("(新)ロマンティック", zetsurin);
        let zetsurin_superstar = make_track("スーパースター", zetsurin);
        let zetsurin_kagura = make_track("神楽0", zetsurin);
        let zetsurin_zetsurin = make_track("絶倫パトロール", zetsurin);
        let zetsurin_yokumo = make_track("良くも悪くも。", zetsurin);
        let zetsurin_aino = make_track("愛の消費期限", zetsurin);
        let zetsurin_kagura_off = make_track("神楽0(off vocal)", zetsurin);
        let zetsurin_zetsurin_off = make_track("絶倫パトロール(off vocal)", zetsurin);

        // integrate
        let mut album_table = Integrate::default();
        let mut track_table = Integrate::default();

        let mut album_map = Vec::new();
        let mut track_map = Vec::new();

        let mut join = FKJoinIndexed::new(|_: &TrackId, track: &Track| track.album);

        let mut join_integrate = Integrate::default();

        for album in albums {
            album_map.clear();
            album_table.eval(&album, &mut album_map);
            /*let mut join_output = Vec::new();
            join.eval(&track_map[0], &album, &mut join_output);
            for v in join_output {
                join_integrate.eval(&v, &mut vec![]);
            }*/
        }
        for track in tracks {
            track_map.clear();
            track_table.eval(&track, &mut track_map);
            let mut join_output = Vec::new();
            join.eval(&album_map[0], &track, &mut join_output);
            for v in join_output {
                join_integrate.eval(&v, &mut vec![]);
            }
        }

        for ((album_id,track_id), ZVal { value: (album,track) ,multiplicity }) in join_integrate.state {
            println!("{}: {} ({:+})", album.name, track.name, multiplicity);
        }

        ///////////////////////////////////////////////////////////
    }
}
