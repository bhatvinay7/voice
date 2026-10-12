use crate::models::PropertyListing;
use super::{this_lock, DatabaseStore};

impl DatabaseStore {
    pub fn add_listing(&self, listing: PropertyListing) {
        if let Ok(mut guard) = self.listings.lock() {
            guard.push(listing);
        }
    }

    pub fn update_listing(&self, listing_id: &str, new_listing: PropertyListing) -> bool {
        if let Ok(mut guard) = self.listings.lock() {
            if let Some(listing) = guard.iter_mut().find(|l| l.id == listing_id) {
                *listing = new_listing;
                return true;
            }
        }
        false
    }

    pub fn delete_listing(&self, listing_id: &str) -> bool {
        if let Ok(mut guard) = self.listings.lock() {
            let initial_len = guard.len();
            guard.retain(|l| l.id != listing_id);
            return guard.len() < initial_len;
        }
        false
    }

    pub fn get_listings(&self) -> Vec<PropertyListing> {
        let guard = match this_lock(&self.listings) {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        guard.clone()
    }
}
