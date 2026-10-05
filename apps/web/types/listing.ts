export interface PropertyListing {
  id: string;
  title: string;
  secondary_title: string;
  description: string;
  secondary_description: string;
  city: "Dubai" | "Abu Dhabi";
  location: string;
  price: number;
  listing_type: "rent" | "buy";
  bedrooms: number;
  bathrooms: number;
  area_sqft: number;
  pet_friendly: boolean;
  near_metro: boolean;
  agent_id: string;
  available_slots: string[];
}

export interface SearchFilters {
  query?: string;
  city?: string;
  location?: string;
  max_price?: number;
  bedrooms?: number;
  listing_type?: "rent" | "buy";
}
