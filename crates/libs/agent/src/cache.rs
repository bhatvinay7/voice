use db::PropertyListing;

pub struct AgentContextReasoner;

impl AgentContextReasoner {
    /// Checks whether the user's inquiry is unrelated to real estate, properties, viewing slots, or tenancy procedures.
    pub fn is_unrelated_query(query: &str) -> bool {
        let lower = query.to_lowercase();
        let property_indicators = [
            "property",
            "properties",
            "listing",
            "apartment",
            "villa",
            "penthouse",
            "bedroom",
            "price",
            "pricing",
            "rent",
            "rental",
            "buy",
            "sale",
            "aed",
            "real estate",
            "location",
            "available",
            "availability",
            "deposit",
            "viewing",
            "landlord",
            "tenant",
        ];
        if property_indicators.iter().any(|term| lower.contains(term)) {
            return false;
        }
        let unrelated_indicators = [
            "recipe",
            "cook",
            "pasta",
            "cake",
            "ingredients",
            "baking",
            "python",
            "javascript",
            "rust code",
            "write a code",
            "write code",
            "html",
            "css",
            "programming",
            "function",
            "compile error",
            "debug code",
            "who won",
            "world cup",
            "football match",
            "basketball",
            "nba",
            "soccer",
            "capital of",
            "weather in tokyo",
            "weather in london",
            "weather in new york",
            "weather in paris",
            "quantum physics",
            "astronomy",
            "dinosaur",
            "solve equation",
            "calculate 2+",
            "president of",
            "prime minister",
            "election result",
            "tell me a joke",
            "write a poem",
            "sing a song",
        ];

        unrelated_indicators.iter().any(|&k| lower.contains(k))
    }

    /// Determines whether the user is explicitly asking to search a different location or different property.
    pub fn is_different_location_or_new_search(
        user_prompt: &str,
        listing: &PropertyListing,
    ) -> bool {
        let lower = user_prompt.to_lowercase();
        let is_different_location = (lower.contains("downtown")
            && !listing.location.to_lowercase().contains("downtown"))
            || (lower.contains("marina") && !listing.location.to_lowercase().contains("marina"))
            || (lower.contains("palm") && !listing.location.to_lowercase().contains("palm"))
            || (lower.contains("business bay")
                && !listing.location.to_lowercase().contains("business bay"))
            || (lower.contains("creek") && !listing.location.to_lowercase().contains("creek"))
            || (lower.contains("hills") && !listing.location.to_lowercase().contains("hills"))
            || (lower.contains("villa") && !listing.title.to_lowercase().contains("villa"))
            || (lower.contains("penthouse") && !listing.title.to_lowercase().contains("penthouse"));

        let is_explicit_new_search = lower.contains("other property")
            || lower.contains("different property")
            || lower.contains("other options")
            || lower.contains("show me something else")
            || lower.contains("another apartment")
            || lower.contains("another villa")
            || lower.contains("search for");

        is_different_location || is_explicit_new_search
    }

    /// Evaluates whether the agent has enough context from cached data to answer the user question directly.
    /// Returns Some(answer) if answered from cache, or None if a new search/tool call is required.
    pub fn answer_from_cached_context(
        listing: &PropertyListing,
        user_prompt: &str,
    ) -> Option<String> {
        let lower = user_prompt.to_lowercase();

        // 1. Check if the user is asking for a completely DIFFERENT search or location.
        // Scope checks happen after retrieval so this cache shortcut never blocks RAG.
        if Self::is_different_location_or_new_search(&lower, listing) {
            return None;
        }

        // 3. Defer statutory regulatory legal inquiries to dynamic RAG tool retrieval
        if lower.contains("procedure")
            || lower.contains("how to buy")
            || lower.contains("buying process")
            || lower.contains("steps to buy")
            || lower.contains("form f")
            || lower.contains("mou")
            || lower.contains("dld fee")
            || lower.contains("golden visa")
            || lower.contains("transfer fee")
            || lower.contains("noc")
        {
            return None;
        }

        // 4. Edge Question: Availability, Vacancy, Market Status, Ready to Move ("is it available or not")
        // Reason directly from already fetched data instead of tool calling again
        let is_availability_query = lower.contains("available")
            || lower.contains("availabl")
            || lower.contains("vacant")
            || lower.contains("occupancy")
            || lower.contains("occupied")
            || lower.contains("on the market")
            || lower.contains("still on sale")
            || lower.contains("still for rent")
            || lower.contains("ready to move")
            || lower.contains("can i move")
            || lower.contains("is it taken")
            || lower.contains("status of")
            || lower.contains("available or not")
            || lower.contains("is it free")
            || lower.contains("current status");

        if is_availability_query {
            let slots_str = if listing.available_slots.is_empty() {
                "Viewing appointments can be arranged upon request on weekdays between 10:00 AM and 6:00 PM.".to_string()
            } else {
                format!(
                    "Open viewing slots are available on: {}.",
                    listing.available_slots.join(", ")
                )
            };
            return Some(format!(
                "Yes, {} in {} is currently active and available for {}. {}. Would you like me to reserve a viewing slot for you?",
                listing.title, listing.location, listing.listing_type, slots_str
            ));
        }

        // 5. Question about Rent / Price / Cost / Deposit / Cheques / Payment Terms
        if lower.contains("rent")
            || lower.contains("price")
            || lower.contains("cost")
            || lower.contains("how much")
            || lower.contains("rate")
            || lower.contains("fee")
            || lower.contains("budget")
            || lower.contains("cheque")
            || lower.contains("payment")
            || lower.contains("deposit")
            || lower.contains("installment")
        {
            let pricing_str = if listing.listing_type == "buy" {
                format!(
                    "The purchase price for {} in {} is AED {}. Standard escrow milestones and registration terms apply.",
                    listing.title, listing.location, listing.price_aed
                )
            } else {
                format!(
                    "The annual rent for {} in {} is AED {} per year. In the UAE, rental payments are typically scheduled in 1 to 4 cheques.",
                    listing.title, listing.location, listing.price_aed
                )
            };
            return Some(pricing_str);
        }

        // 6. Question about Listing Type (Rent vs Buy)
        if lower.contains("is it for rent")
            || lower.contains("is it for sale")
            || lower.contains("is it for buy")
            || lower.contains("buy or rent")
            || lower.contains("rental or sale")
        {
            return Some(format!(
                "{} is listed exclusively for {} at AED {}. It is located in {}.",
                listing.title, listing.listing_type, listing.price_aed, listing.location
            ));
        }

        // 7. Question about Bedrooms / Bathrooms / Layout / Rooms
        if lower.contains("bedroom")
            || lower.contains("bed")
            || lower.contains("bath")
            || lower.contains("how many room")
            || lower.contains("number of room")
            || lower.contains("layout")
            || lower.contains("studio")
            || lower.contains("bhk")
        {
            return Some(format!(
                "{} features {} bedrooms and {} bathrooms, offering {} sq ft of spacious luxury living.",
                listing.title, listing.bedrooms, listing.bathrooms, listing.area_sqft
            ));
        }

        // 8. Question about Area / Size / Square Footage
        if lower.contains("area")
            || lower.contains("size")
            || lower.contains("sqft")
            || lower.contains("square")
            || lower.contains("big")
            || lower.contains("dimensions")
            || lower.contains("spacious")
        {
            return Some(format!(
                "The total area of {} is {} sq ft.",
                listing.title, listing.area_sqft
            ));
        }

        // 9. Question about Location / Address / Metro / Transport / Neighborhood
        if lower.contains("where")
            || lower.contains("location")
            || lower.contains("address")
            || lower.contains("metro")
            || lower.contains("neighborhood")
            || lower.contains("near")
            || lower.contains("commute")
            || lower.contains("station")
            || lower.contains("transit")
        {
            let metro_status = if listing.near_metro {
                "It is located conveniently within walking distance of the Dubai Metro station."
            } else {
                "It provides quick vehicular access to major Dubai arterial roads and attractions."
            };
            return Some(format!(
                "{} is located in {}, {}. {}",
                listing.title, listing.location, listing.city, metro_status
            ));
        }

        // 10. Question about Pets / Pet Policy
        if lower.contains("pet")
            || lower.contains("dog")
            || lower.contains("cat")
            || lower.contains("animal")
        {
            let pet_note = if listing.pet_friendly {
                "Yes, this building is pet-friendly."
            } else {
                "Pets are unfortunately not permitted in this residential building."
            };
            return Some(format!(
                "{} Regarding {} in {}.",
                pet_note, listing.title, listing.location
            ));
        }

        // 11. Question about Furnishing Status
        if lower.contains("furnished")
            || lower.contains("unfurnished")
            || lower.contains("furniture")
            || lower.contains("fitted")
            || lower.contains("appliance")
        {
            let desc_low =
                format!("{} {}", listing.description, listing.secondary_description).to_lowercase();
            let furnished_status = if desc_low.contains("unfurnished") {
                "is offered unfurnished, ready for custom personalization."
            } else if desc_low.contains("furnished") {
                "is offered fully furnished with top-tier designer finishes."
            } else {
                "features high-end fittings and luxurious architectural specifications."
            };
            return Some(format!(
                "{} in {} {}. Description: {}.",
                listing.title, listing.location, furnished_status, listing.description
            ));
        }

        // 12. Question about View / Orientation / Balcony
        if lower.contains("view")
            || lower.contains("burj")
            || lower.contains("sea view")
            || lower.contains("marina view")
            || lower.contains("skyline")
            || lower.contains("facing")
            || lower.contains("outlook")
        {
            return Some(format!(
                "{} in {} boasts outstanding views: {}.",
                listing.title, listing.location, listing.description
            ));
        }

        // 13. Question about Floor / Level / High Floor
        if lower.contains("floor")
            || lower.contains("level")
            || lower.contains("story")
            || lower.contains("storey")
            || lower.contains("high floor")
        {
            return Some(format!(
                "{} in {} is situated on a prime floor level. Overview: {}.",
                listing.title, listing.location, listing.description
            ));
        }

        // 14. Question about Maintenance / Chiller / AC / Utilities
        if lower.contains("chiller")
            || lower.contains("ac")
            || lower.contains("air condition")
            || lower.contains("maintenance")
            || lower.contains("service charge")
            || lower.contains("utility")
            || lower.contains("dewa")
            || lower.contains("bill")
        {
            return Some(format!(
                "For {} in {}, dedicated on-site residential maintenance and concierge services are provided. Standard DEWA utility and community guidelines apply.",
                listing.title, listing.location
            ));
        }

        // 15. Question about Parking / Garage
        if lower.contains("parking")
            || lower.contains("garage")
            || lower.contains("car park")
            || lower.contains("parking space")
        {
            return Some(format!(
                "Yes, {} in {} includes dedicated reserved covered parking for residents.",
                listing.title, listing.location
            ));
        }

        // 16. Question about Balcony / Terrace
        if lower.contains("balcony")
            || lower.contains("terrace")
            || lower.contains("patio")
            || lower.contains("veranda")
        {
            return Some(format!(
                "Yes, {} includes private balcony space offering expansive views across {}.",
                listing.title, listing.location
            ));
        }

        // 17. Question about Amenities / Facilities / Pool / Gym / Concierge
        if lower.contains("amenit")
            || lower.contains("facilit")
            || lower.contains("pool")
            || lower.contains("gym")
            || lower.contains("security")
            || lower.contains("concierge")
        {
            return Some(format!(
                "Key amenities for {} include reserved covered parking, state-of-the-art gymnasium, outdoor swimming pool, and 24-hour concierge security.",
                listing.title
            ));
        }

        // 18. Question about Property Reference ID or Agent
        if lower.contains("ref")
            || lower.contains("reference")
            || lower.contains("property id")
            || lower.contains("listing id")
            || lower.contains("agent")
            || lower.contains("broker")
        {
            return Some(format!(
                "The reference ID for {} is {} (Managing Advisor ID: {}).",
                listing.title, listing.id, listing.agent_id
            ));
        }

        // 19. Question about Available Viewing Slots / Times
        if (lower.contains("when")
            && (lower.contains("view") || lower.contains("visit") || lower.contains("see")))
            || lower.contains("available slot")
            || lower.contains("timings")
        {
            let slots_text = if listing.available_slots.is_empty() {
                "Weekdays between 10:00 AM and 6:00 PM".to_string()
            } else {
                listing.available_slots.join(", ")
            };
            return Some(format!(
                "Available viewing slots for {} are: {}. Would you like me to reserve a viewing slot for you?",
                listing.title, slots_text
            ));
        }

        // 20. Question asking for details / overview / description / suitability
        if lower.contains("detail")
            || lower.contains("tell me more")
            || lower.contains("more info")
            || lower.contains("describe")
            || lower.contains("about it")
            || lower.contains("overview")
            || lower.contains("explain")
            || lower.contains("suitab")
            || lower.contains("family")
            || lower.contains("why should")
            || lower.contains("summary")
        {
            return Some(format!(
                "{}. Located in {}, priced at AED {} for {} with {} bedrooms and {} sq ft.",
                listing.description,
                listing.location,
                listing.price_aed,
                listing.listing_type,
                listing.bedrooms,
                listing.area_sqft
            ));
        }

        None
    }
}
