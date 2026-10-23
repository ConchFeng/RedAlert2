//! Shelf allocation and sub-rectangle uploads for atlas pages that grow.
//!
//! The entity atlases pack everything the map starts with once, at load.
//! Sprites that appear later — the first building or vehicle of a new type
//! mid-match, a slope-transition frame — go into free shelf space on a page
//! sized to the refresh that opened it, and upload only the rows they occupy
//! in bands, so adding sprites never repacks or re-uploads the resident ones.

/// Gap between neighbouring sprites so sampling never bleeds across them.
pub(crate) const SPRITE_PADDING: u32 = 1;

/// Next free position on a page that is filled shelf by shelf, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShelfCursor {
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    shelf_height: u32,
}

impl ShelfCursor {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            x: 0,
            y: 0,
            shelf_height: 0,
        }
    }

    /// Close the current shelf, so the next placement starts on rows no
    /// earlier sprite occupies.
    pub(crate) fn start_new_shelf(&mut self) {
        if self.x > 0 {
            self.y = self.y.saturating_add(self.shelf_height + SPRITE_PADDING);
            self.x = 0;
            self.shelf_height = 0;
        }
    }

    /// Reserve a `width` x `height` rectangle and return its top-left corner.
    /// A full page returns `None` and keeps its cursor, so a smaller sprite
    /// can still use the current shelf.
    pub(crate) fn place(&mut self, width: u32, height: u32) -> Option<[u32; 2]> {
        if width > self.width || height > self.height {
            return None;
        }
        let (mut x, mut y, mut shelf_height) = (self.x, self.y, self.shelf_height);
        if x.saturating_add(width) > self.width {
            y = y.saturating_add(shelf_height + SPRITE_PADDING);
            x = 0;
            shelf_height = 0;
        }
        if y.saturating_add(height) > self.height {
            return None;
        }
        self.x = x + width + SPRITE_PADDING;
        self.y = y;
        self.shelf_height = shelf_height.max(height);
        Some([x, y])
    }
}

/// The page an atlas appends to, and its shelf.
pub(crate) struct GrowthShelf {
    pub(crate) page: usize,
    pub(crate) shelf: ShelfCursor,
}

/// One refresh's sprites on one page. Their rows hold no earlier sprite, so
/// each texture of the page can upload them in one or more [`write_band`]s
/// without overwriting resident sprites.
pub(crate) struct PagePlacements<T> {
    pub(crate) page: usize,
    pub(crate) sprites: Vec<([u32; 2], T)>,
}

/// Place one refresh's sprites, tallest first, on the current growth page,
/// opening a page with `open_page` whenever it is full. `open_page` gets the
/// [`suggested_page_size`] for the sprites that still need room — at least the
/// one being placed, between `floor` and `cap` — and returns the new page's
/// index and size. The refresh starts on a fresh shelf, so the rows it fills
/// hold no resident sprite. Sprites wider or taller than `max_dim` are
/// returned unplaced.
pub(crate) fn place_on_growth_pages<T>(
    growth: &mut Option<GrowthShelf>,
    mut sprites: Vec<T>,
    size: impl Fn(&T) -> [u32; 2],
    floor: u32,
    cap: u32,
    max_dim: u32,
    mut open_page: impl FnMut([u32; 2]) -> (usize, [u32; 2]),
) -> (Vec<PagePlacements<T>>, Vec<T>) {
    let mut placed: Vec<PagePlacements<T>> = Vec::new();
    let mut unplaced = Vec::new();
    if sprites.is_empty() {
        return (placed, unplaced);
    }
    sprites.sort_by_key(|sprite| std::cmp::Reverse(size(sprite)[1]));
    // The shelves fill tallest first, so the rows a new page still has to
    // hold are exactly the sprites from the current one onward.
    let sizes: Vec<[u32; 2]> = sprites.iter().map(&size).collect();
    if let Some(growth) = growth.as_mut() {
        growth.shelf.start_new_shelf();
    }
    for (index, sprite) in sprites.into_iter().enumerate() {
        let [width, height] = size(&sprite);
        if width > max_dim || height > max_dim {
            unplaced.push(sprite);
            continue;
        }
        let spot = growth
            .as_mut()
            .and_then(|growth| Some((growth.page, growth.shelf.place(width, height)?)));
        let (page, origin) = match spot {
            Some(spot) => spot,
            None => {
                let (page, [page_width, page_height]) =
                    open_page(suggested_page_size(&sizes[index..], floor, cap, max_dim));
                let mut shelf = ShelfCursor::new(page_width, page_height);
                let origin = shelf.place(width, height);
                *growth = Some(GrowthShelf { page, shelf });
                match origin {
                    Some(origin) => (page, origin),
                    None => {
                        unplaced.push(sprite);
                        continue;
                    }
                }
            }
        };
        match placed.last_mut() {
            Some(group) if group.page == page => group.sprites.push((origin, sprite)),
            _ => placed.push(PagePlacements {
                page,
                sprites: vec![(origin, sprite)],
            }),
        }
    }
    (placed, unplaced)
}

/// Page size for a refresh that still has to place `sizes`, tallest sprite
/// first as [`place_on_growth_pages`] does.
///
/// A growth page used to be one fixed square edge — 8192 for the unit atlas,
/// 64 MiB of R8 texels — whatever the refresh needed: a campaign's 32-sprite
/// refresh paid the same as a 21,000-sprite one, and the allocation with its
/// zero-fill landed on the frame that triggered the refresh. The estimate here
/// is the area the sprites need plus half again, so later refreshes find free
/// shelves and keep appending instead of each opening a page. The edge is that
/// area's square side, widened until the shelves fit, as the map-load pack
/// does, and the rows are what the shelves then hold.
///
/// `floor_side` and `cap_side` bound it. The floor keeps many small refreshes
/// sharing one page, which bounds the page count an atlas's entries index.
/// The cap keeps a large refresh at the page the game already allocated, so
/// sizing can only shrink a page. A sprite bigger than either still raises the
/// edge to what it needs, and past `max_dim` the placer reports it as before.
pub(crate) fn suggested_page_size(
    sizes: &[[u32; 2]],
    floor_side: u32,
    cap_side: u32,
    max_dim: u32,
) -> [u32; 2] {
    if sizes.is_empty() || max_dim == 0 {
        return [0, 0];
    }
    // A sprite no page could hold is unplaced by the caller; it must not
    // inflate the page for the sprites that do fit.
    let sizes: Vec<[u32; 2]> = sizes
        .iter()
        .filter(|[width, height]| *width <= max_dim && *height <= max_dim)
        .copied()
        .collect();
    if sizes.is_empty() {
        return [0, 0];
    }
    let widest = sizes.iter().map(|size| size[0]).max().unwrap_or(0) as u64;
    let tallest = sizes.iter().map(|size| size[1]).max().unwrap_or(0) as u64;
    // Each sprite costs its rows plus one padded column beside it.
    let area = sizes
        .iter()
        .map(|[width, height]| (u64::from(*width) + u64::from(SPRITE_PADDING)) * u64::from(*height))
        .sum::<u64>();
    let ceiling = u64::from(cap_side)
        .max(widest.max(tallest))
        .min(u64::from(max_dim));
    let with_room = area + area / 2;
    let mut width = with_room
        .isqrt()
        .max(widest)
        .max(u64::from(floor_side))
        .min(ceiling)
        .max(1);
    let mut rows = shelf_height(&sizes, width);
    while rows > width && width < ceiling {
        width = (width * 2).min(ceiling);
        rows = shelf_height(&sizes, width);
    }
    let width = width.min(ceiling) as u32;
    // Rows past the ceiling spill to the next page the placer opens.
    let height = rows.max(tallest).min(ceiling) as u32;
    [width, height].map(|edge| edge.max(floor_side).min(max_dim))
}

/// Shelf height `sizes` reach at `page_width`, filled left to right and
/// tallest first, exactly as [`place_on_growth_pages`] places them.
fn shelf_height(sizes: &[[u32; 2]], page_width: u64) -> u64 {
    let pad = u64::from(SPRITE_PADDING);
    let (mut x, mut y, mut shelf) = (0u64, 0u64, 0u64);
    for &[width, height] in sizes {
        let (width, height) = (u64::from(width), u64::from(height));
        if x + width > page_width {
            y += shelf + pad;
            x = 0;
            shelf = 0;
        }
        x += width + pad;
        shelf = shelf.max(height);
    }
    y + shelf
}

/// Split one page group's placements into bands of whole shelves, each
/// staging no more than `band_bytes` of texels, and pass every band's sprites
/// to `write`. `sprites` runs shelf by shelf from the top, as
/// [`place_on_growth_pages`] returns it. Gathering a whole refresh in one pass
/// stages every row it spans at once — tens of megabytes for a large one —
/// the same allocation stall the fixed-size page used to cause. `row_bytes` is
/// the destination stride: [`gather_band`] writes from column 0 to the band's
/// rightmost texel, so a band costs its rows times that stride.
pub(crate) fn write_group_bands<T>(
    sprites: &[([u32; 2], T)],
    size: impl Fn(&T) -> [u32; 2],
    row_bytes: u64,
    band_bytes: u64,
    mut write: impl FnMut(&[([u32; 2], &T)]),
) {
    debug_assert!(
        sprites.windows(2).all(|pair| pair[0].0[1] <= pair[1].0[1]),
        "placements must run shelf by shelf from the top"
    );
    let mut start = 0usize;
    while start < sprites.len() {
        let top = sprites[start].0[1];
        let (mut end, mut bottom) = (start, top);
        while end < sprites.len() {
            // Take one whole shelf, so a band never splits a shelf's rows.
            let shelf_y = sprites[end].0[1];
            let mut next = end;
            while next < sprites.len() && sprites[next].0[1] == shelf_y {
                bottom = bottom.max(sprites[next].0[1] + size(&sprites[next].1)[1]);
                next += 1;
            }
            if end > start && u64::from(bottom - top) * row_bytes > band_bytes {
                break;
            }
            end = next;
        }
        let band: Vec<_> = sprites[start..end]
            .iter()
            .map(|(origin, sprite)| (*origin, sprite))
            .collect();
        write(&band);
        start = end;
    }
}

/// Rectangles `(origin, size)` gathered into the band of rows they span, from
/// column 0 to the rightmost texel: returns the band's origin, size and
/// tightly packed texels. `texels(i)` gives rectangle `i`'s tightly packed
/// texels as it is copied in, so a caller can produce them one at a time.
/// Texels no rectangle covers are zero.
pub(crate) fn gather_band<T: AsRef<[u8]>>(
    rects: &[([u32; 2], [u32; 2])],
    bytes_per_texel: u32,
    mut texels: impl FnMut(usize) -> T,
) -> Option<([u32; 2], [u32; 2], Vec<u8>)> {
    let top = rects.iter().map(|(origin, _)| origin[1]).min()?;
    let bottom = rects
        .iter()
        .map(|(origin, size)| origin[1] + size[1])
        .max()?;
    let right = rects
        .iter()
        .map(|(origin, size)| origin[0] + size[0])
        .max()?;
    let texel = bytes_per_texel as usize;
    let stride = right as usize * texel;
    let mut band = vec![0u8; stride * (bottom - top) as usize];
    for (i, (origin, size)) in rects.iter().enumerate() {
        let texels = texels(i);
        let texels = texels.as_ref();
        let row = size[0] as usize * texel;
        for y in 0..size[1] as usize {
            let start =
                (origin[1] - top) as usize * stride + y * stride + origin[0] as usize * texel;
            band[start..start + row].copy_from_slice(&texels[y * row..(y + 1) * row]);
        }
    }
    Some(([0, top], [right, bottom - top], band))
}

/// Upload rectangles placed on rows no resident sprite occupies (a fresh
/// shelf, see [`ShelfCursor::start_new_shelf`]) with one write, however many
/// there are; per-rectangle writes cost a staging allocation each.
pub(crate) fn write_band<T: AsRef<[u8]>>(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    bytes_per_texel: u32,
    rects: &[([u32; 2], [u32; 2])],
    texels: impl FnMut(usize) -> T,
) {
    if let Some((origin, size, band)) = gather_band(rects, bytes_per_texel, texels) {
        write_texels(queue, texture, origin, size, bytes_per_texel, &band);
    }
}

/// Upload one tightly packed rectangle of texels into `texture` at `origin`.
pub(crate) fn write_texels(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    origin: [u32; 2],
    size: [u32; 2],
    bytes_per_texel: u32,
    texels: &[u8],
) {
    let [width, height] = size;
    if width == 0 || height == 0 {
        return;
    }
    debug_assert_eq!(
        texels.len(),
        (width * height * bytes_per_texel) as usize,
        "texel rectangle must be tightly packed"
    );
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: origin[0],
                y: origin[1],
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * bytes_per_texel),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelves_fill_left_to_right_then_wrap_below_the_tallest_sprite() {
        let mut shelf = ShelfCursor::new(10, 10);
        assert_eq!(shelf.place(4, 3), Some([0, 0]));
        assert_eq!(shelf.place(4, 2), Some([5, 0]));
        // 10 + 4 > 10: wraps below the first shelf's tallest sprite plus padding.
        assert_eq!(shelf.place(4, 2), Some([0, 4]));
    }

    #[test]
    fn a_full_page_refuses_without_losing_its_current_shelf() {
        let mut shelf = ShelfCursor::new(10, 6);
        assert_eq!(shelf.place(4, 4), Some([0, 0]));
        // A new shelf would start at y = 5 and cannot hold 4 rows.
        assert_eq!(shelf.place(8, 4), None);
        // The refusal did not wrap the cursor: the first shelf still has room.
        assert_eq!(shelf.place(4, 4), Some([5, 0]));
        assert_eq!(shelf.place(11, 1), None, "wider than the page");
    }

    #[test]
    fn a_new_shelf_starts_below_every_placed_sprite() {
        let mut shelf = ShelfCursor::new(10, 10);
        assert_eq!(shelf.place(2, 3), Some([0, 0]));
        shelf.start_new_shelf();
        assert_eq!(shelf.place(2, 1), Some([0, 4]));
        shelf.start_new_shelf();
        shelf.start_new_shelf();
        assert_eq!(
            shelf.place(2, 1),
            Some([0, 6]),
            "an empty shelf is not skipped"
        );
    }

    #[test]
    fn a_band_gathers_rectangles_into_the_rows_they_span() {
        let tall = [1u8, 2, 3, 4, 5, 6];
        let wide = [7u8, 8, 9];
        let texels = [&tall[..], &wide[..]];
        let (origin, size, band) =
            gather_band(&[([0, 4], [2, 3]), ([3, 4], [3, 1])], 1, |i| texels[i])
                .expect("two rectangles");
        assert_eq!((origin, size), ([0, 4], [6, 3]));
        #[rustfmt::skip]
        assert_eq!(band, vec![
            1, 2, 0, 7, 8, 9,
            3, 4, 0, 0, 0, 0,
            5, 6, 0, 0, 0, 0,
        ]);
        assert!(gather_band(&[], 4, |_| [0u8; 0]).is_none());
    }

    fn bottom(placed: &[PagePlacements<[u32; 2]>]) -> u32 {
        placed
            .iter()
            .flat_map(|group| &group.sprites)
            .map(|(origin, size)| origin[1] + size[1])
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn a_small_refresh_asks_for_a_page_its_sprites_actually_need() {
        // One vehicle type's every-slope sprite set: the unit atlas used to
        // answer this with a fixed 8192 edge, 64 MiB of R8 texels plus their
        // zero-fill, on the frame that first drew the type.
        let sprites = [[120u32, 130u32]; 32 * 17];
        let [width, height] = suggested_page_size(&sprites, 2048, 8192, 16384);
        assert!(
            u64::from(width) * u64::from(height) < 8192 * 8192 / 2,
            "32x17 sprites of 120x130 ask for {width}x{height}"
        );
        // The page still holds them: shelves tallest first inside `width`.
        assert!(shelf_height(&sprites, u64::from(width)) <= u64::from(height.max(width)));
        assert!(width >= height, "a page is never a column of shelves");

        // A refresh smaller than the floor gets the floor, so many of them
        // share one page instead of each opening one.
        assert_eq!(
            suggested_page_size(&[[32, 40]], 2048, 8192, 16384),
            [2048, 2048]
        );
    }

    #[test]
    fn a_large_refresh_never_asks_for_more_than_the_old_fixed_page() {
        // A cap of 8192 is the edge every unit growth page used to have, so
        // sizing only shrinks a page: a refresh bigger than that still gets
        // the whole old page, and a sprite bigger than the cap but admissible
        // still raises the edge to what it needs.
        let many = [[400u32, 400u32]; 400];
        let [width, height] = suggested_page_size(&many, 2048, 8192, 16384);
        assert_eq!(width, 8192, "a refresh past the cap gets the old edge");
        assert!(
            u64::from(width) * u64::from(height) <= 8192 * 8192,
            "never more texels than the old page: {width}x{height}"
        );
        assert_eq!(
            suggested_page_size(&[[12000, 40]], 2048, 8192, 16384),
            [12000, 2048],
            "a wide sprite is never squeezed below its own width"
        );
        // Sprites the device cannot hold at all are left to the placer, which
        // reports them; they do not size the page for the rest.
        assert_eq!(
            suggested_page_size(&[[12000, 40], [100, 100]], 2048, 8192, 8192),
            [2048, 2048],
            "only admissible sprites count"
        );
        assert_eq!(suggested_page_size(&[], 2048, 8192, 16384), [0, 0]);
    }

    #[test]
    fn growth_bands_take_whole_shelves_and_stay_inside_their_budget() {
        // Three shelves of eight 100x100 sprites on a page 1000 texels wide.
        let placed: Vec<([u32; 2], [u32; 2])> = (0..3)
            .flat_map(|shelf| {
                (0..8).map(move |column| ([column * 100, shelf * 101], [100u32, 100u32]))
            })
            .collect();
        let mut bands: Vec<Vec<[u32; 2]>> = Vec::new();
        // One shelf spans 100 rows of a 1000-texel stride: 100 KB staged.
        write_group_bands(
            &placed,
            |s| *s,
            1000,
            250_000,
            |band| {
                bands.push(band.iter().map(|(origin, _)| *origin).collect());
            },
        );
        assert_eq!(
            bands.iter().map(|band| band.len()).collect::<Vec<_>>(),
            [16, 8],
            "two shelves, then the one that would not fit: {bands:?}"
        );
        let written: usize = bands.iter().map(|band| band.len()).sum();
        assert_eq!(written, placed.len(), "every placement is written once");
        assert!(
            bands.iter().all(|band| band
                .windows(2)
                .all(|p| p[0][1] == p[1][1] || p[0][1] < p[1][1])),
            "a band holds whole shelves in page order"
        );
        // A budget smaller than one shelf still writes it, so a band is never
        // empty and the loop always advances.
        let mut single = 0;
        write_group_bands(&placed, |s| *s, 1000, 1, |_| single += 1);
        assert_eq!(single, 3, "one band per shelf");
    }

    #[test]
    fn a_refresh_band_never_reaches_rows_an_earlier_refresh_filled() {
        let mut growth = None;
        let mut opened = Vec::new();
        let mut open_page = |needed: [u32; 2]| {
            opened.push(needed);
            (opened.len() + 1, [16, 16])
        };
        let (first, unplaced) = place_on_growth_pages(
            &mut growth,
            vec![[5, 2], [3, 4]],
            |s| *s,
            0,
            64,
            64,
            &mut open_page,
        );
        assert!(unplaced.is_empty());
        assert_eq!(first.len(), 1, "one page");
        // The earlier shelf still has room for a 3x1 sprite at x = 10; a
        // refresh must not use it, since its band would zero those rows.
        let (second, _) =
            place_on_growth_pages(&mut growth, vec![[3, 1]], |s| *s, 0, 64, 64, &mut open_page);
        let top = second[0]
            .sprites
            .iter()
            .map(|(origin, _)| origin[1])
            .min()
            .unwrap();
        assert!(
            top >= bottom(&first),
            "band top {top} below {}",
            bottom(&first)
        );
        assert_eq!(second[0].page, first[0].page);
        assert_eq!(
            opened,
            vec![[12, 4]],
            "the first refresh asked for the page its sprites need, not a 64 MiB square"
        );
    }

    #[test]
    fn a_full_page_opens_the_next_and_oversized_sprites_stay_unplaced() {
        let mut growth = None;
        let mut pages = 0;
        let (placed, unplaced) = place_on_growth_pages(
            &mut growth,
            vec![[8, 8], [8, 8], [20, 1]],
            |s| *s,
            0,
            16,
            16,
            |_| {
                pages += 1;
                (pages, [8, 8])
            },
        );
        assert_eq!(unplaced, vec![[20, 1]]);
        let pages_used: Vec<usize> = placed.iter().map(|group| group.page).collect();
        assert_eq!(pages_used, vec![1, 2], "the second 8x8 needs its own page");
        let (none, unplaced) = place_on_growth_pages(
            &mut growth,
            Vec::<[u32; 2]>::new(),
            |s| *s,
            0,
            16,
            16,
            |_| unreachable!(),
        );
        assert!(none.is_empty() && unplaced.is_empty());
    }
}
