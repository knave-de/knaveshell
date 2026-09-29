//! Bounded icon cache for search rows.
use knave_icons::IconLookup;
use knave_ui::UiImage;
use std::collections::{HashMap, VecDeque};

/// Rasterized at twice the 32px row icon so it stays sharp at scale 2.
pub(super) const RASTER_PX: u32 = 64;
const MAX_CACHED: usize = 128;
/// A worst-case SVG takes about 10ms; this keeps one frame well under a display refresh budget.
const LOADS_PER_FRAME: usize = 4;
pub(super) const FALLBACK: &str = "application-x-executable";

pub(super) struct Icons {
    lookup: IconLookup,
    /// `None` records a name with no usable icon so it is not searched again.
    cache: HashMap<String, Option<UiImage>>,
    order: VecDeque<String>,
}

impl Icons {
    pub fn new(lookup: IconLookup) -> Self {
        Self {
            lookup,
            cache: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&UiImage> {
        self.cache.get(name)?.as_ref()
    }

    /// Decode up to `LOADS_PER_FRAME` uncached names. Returns true when names remain.
    pub fn fill<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) -> bool {
        let mut loaded = 0;
        for name in names {
            if self.cache.contains_key(name) {
                continue;
            }
            if loaded == LOADS_PER_FRAME {
                return true;
            }
            loaded += 1;
            let image = self.decode(name);
            self.insert(name.to_owned(), image);
        }
        false
    }

    fn decode(&mut self, name: &str) -> Option<UiImage> {
        match self.lookup.load(name, RASTER_PX) {
            Ok(Some(icon)) => UiImage::from_rgba(icon.width, icon.height, icon.rgba),
            Ok(None) => self.fallback(name),
            Err(error) => {
                eprintln!("knave-shell: {error}");
                self.fallback(name)
            }
        }
    }

    fn fallback(&mut self, name: &str) -> Option<UiImage> {
        if name == FALLBACK {
            return None;
        }
        if !self.cache.contains_key(FALLBACK) {
            let image = self.decode(FALLBACK);
            self.insert(FALLBACK.to_owned(), image);
        }
        self.cache.get(FALLBACK)?.clone()
    }

    fn insert(&mut self, name: String, image: Option<UiImage>) {
        while self.order.len() >= MAX_CACHED {
            if let Some(oldest) = self.order.pop_front() {
                self.cache.remove(&oldest);
            }
        }
        self.order.push_back(name.clone());
        self.cache.insert(name, image);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.cache.len()
    }
}
