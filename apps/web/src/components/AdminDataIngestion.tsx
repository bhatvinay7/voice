"use client";

import React, { useState, useEffect } from "react";
import { useAdminAddListing, useAdminUpdateListing, useAdminDeleteListing, useListings } from "../lib/api";
import { PropertyListing } from "../../types/listing";
import { formatNumber } from "../lib/formatters";

export function AdminDataIngestion(): React.ReactElement {
  const { data: listingsData, isLoading: isLoadingListings } = useListings();
  const addListingMutation = useAdminAddListing();
  const updateListingMutation = useAdminUpdateListing();
  const deleteListingMutation = useAdminDeleteListing();

  const [isEditing, setIsEditing] = useState<boolean>(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  // Form State
  const [title, setTitle] = useState<string>("");
  const [secondaryTitle, setSecondaryTitle] = useState<string>("");
  const [description, setDescription] = useState<string>("");
  const [secondaryDescription, setSecondaryDescription] = useState<string>("");
  const [city, setCity] = useState<string>("Dubai");
  const [location, setLocation] = useState<string>("Marina Heights");
  const [priceInput, setPriceInput] = useState<number>(110000);
  const [listingType, setListingType] = useState<string>("rent");
  const [bedrooms, setBedrooms] = useState<number>(2);
  const [bathrooms, setBathrooms] = useState<number>(2);
  const [areaSqft, setAreaSqft] = useState<number>(1200);
  const [petFriendly, setPetFriendly] = useState<boolean>(true);
  const [nearMetro, setNearMetro] = useState<boolean>(true);

  const resetForm = () => {
    setTitle("");
    setSecondaryTitle("");
    setDescription("");
    setSecondaryDescription("");
    setCity("Dubai");
    setLocation("Marina Heights");
    setPriceInput(110000);
    setListingType("rent");
    setBedrooms(2);
    setBathrooms(2);
    setAreaSqft(1200);
    setPetFriendly(true);
    setNearMetro(true);
    setIsEditing(false);
    setEditingId(null);
  };

  const handleEdit = (listing: PropertyListing) => {
    setIsEditing(true);
    setEditingId(listing.id);
    setTitle(listing.title);
    setSecondaryTitle(listing.secondary_title || "");
    setDescription(listing.description);
    setSecondaryDescription(listing.secondary_description || "");
    setCity(listing.city || "Dubai");
    setLocation(listing.location);
    setPriceInput(listing.price_aed || listing.price || 0);
    setListingType(listing.listing_type || "rent");
    setBedrooms(listing.bedrooms);
    setBathrooms(listing.bathrooms);
    setAreaSqft(listing.area_sqft);
    setPetFriendly(listing.pet_friendly ?? true);
    setNearMetro(listing.near_metro ?? true);
    
    // Smooth scroll to top
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  const handleDelete = (id: string) => {
    if (confirm("Are you sure you want to delete this listing?")) {
      deleteListingMutation.mutate(id);
    }
  };

  const handleSubmit = (e: React.FormEvent): void => {
    e.preventDefault();
    if (!title || !description) return;

    const payload: PropertyListing = {
      id: isEditing && editingId ? editingId : `prop-${Date.now()}`,
      title,
      secondary_title: secondaryTitle,
      description,
      secondary_description: secondaryDescription,
      city,
      location,
      price_aed: Number(priceInput),
      listing_type: listingType,
      bedrooms: Number(bedrooms),
      bathrooms: Number(bathrooms),
      area_sqft: Number(areaSqft),
      pet_friendly: petFriendly,
      near_metro: nearMetro,
      agent_id: (typeof window !== "undefined" && (localStorage.getItem("admin_user_id") || localStorage.getItem("user_id"))) || "agent-admin",
      available_slots: [
        new Date(Date.now() + 86400000).toISOString(),
        new Date(Date.now() + 172800000).toISOString(),
      ],
    };

    if (isEditing && editingId) {
      updateListingMutation.mutate({ id: editingId, listing: payload }, {
        onSuccess: () => resetForm(),
      });
    } else {
      addListingMutation.mutate(payload, {
        onSuccess: () => resetForm(),
      });
    }
  };

  return (
    <div className="space-y-8">
      {/* Form Section */}
      <div className="bg-theme-card border border-theme-border rounded-2xl p-6 shadow-xl relative overflow-hidden">
        <div className="absolute top-0 right-0 w-32 h-32 bg-cyan-500/10 rounded-full blur-3xl -mr-16 -mt-16 pointer-events-none"></div>
        
        <div className="mb-6">
          <h3 className="text-2xl font-bold text-theme-text font-serif">
            {isEditing ? "Update Property Listing" : "Add New Property"}
          </h3>
          <p className="text-sm text-theme-sub mt-1">
            {isEditing 
              ? "Modify the selected property's details. Changes will sync to PostgreSQL and the Vector DB."
              : "Ingest a new real estate listing into the relational store and hybrid Vector DB RAG embeddings index."}
          </p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-6">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {/* Titles */}
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-semibold text-theme-text mb-1">Listing Title <span className="text-red-500">*</span></label>
                <input
                  type="text"
                  required
                  placeholder="e.g. Ultra-Luxury 5BR Villa"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50 transition-shadow"
                />
              </div>
              <div>
                <label className="block text-sm font-semibold text-theme-text mb-1">Secondary Title</label>
                <input
                  type="text"
                  placeholder="e.g. Signature Villa with Private Beach"
                  value={secondaryTitle}
                  onChange={(e) => setSecondaryTitle(e.target.value)}
                  className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50 transition-shadow"
                />
              </div>
            </div>

            {/* Location & Pricing */}
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm font-semibold text-theme-text mb-1">City</label>
                  <select
                    value={city}
                    onChange={(e) => setCity(e.target.value)}
                    className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
                  >
                    <option value="Dubai">Dubai</option>
                    <option value="Abu Dhabi">Abu Dhabi</option>
                    <option value="Sharjah">Sharjah</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-semibold text-theme-text mb-1">Location / District</label>
                  <input
                    type="text"
                    placeholder="e.g. Palm Jumeirah"
                    value={location}
                    onChange={(e) => setLocation(e.target.value)}
                    className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
                  />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm font-semibold text-theme-text mb-1">Price (AED) <span className="text-red-500">*</span></label>
                  <input
                    type="number"
                    required
                    value={priceInput}
                    onChange={(e) => setPriceInput(Number(e.target.value))}
                    className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
                  />
                </div>
                <div>
                  <label className="block text-sm font-semibold text-theme-text mb-1">Listing Type</label>
                  <select
                    value={listingType}
                    onChange={(e) => setListingType(e.target.value)}
                    className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
                  >
                    <option value="rent">Rent</option>
                    <option value="buy">Buy</option>
                  </select>
                </div>
              </div>
            </div>

            {/* Specifications */}
            <div className="md:col-span-2 grid grid-cols-2 md:grid-cols-5 gap-4 bg-theme-main/50 p-4 rounded-xl border border-theme-border/50">
              <div>
                <label className="block text-sm font-semibold text-theme-text mb-1">Bedrooms</label>
                <input
                  type="number"
                  value={bedrooms}
                  onChange={(e) => setBedrooms(Number(e.target.value))}
                  className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
                />
              </div>
              <div>
                <label className="block text-sm font-semibold text-theme-text mb-1">Bathrooms</label>
                <input
                  type="number"
                  value={bathrooms}
                  onChange={(e) => setBathrooms(Number(e.target.value))}
                  className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
                />
              </div>
              <div>
                <label className="block text-sm font-semibold text-theme-text mb-1">Area (sqft)</label>
                <input
                  type="number"
                  value={areaSqft}
                  onChange={(e) => setAreaSqft(Number(e.target.value))}
                  className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-3 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50"
                />
              </div>
              <div className="flex flex-col items-center justify-center pt-5">
                <label className="flex items-center space-x-2 cursor-pointer group">
                  <input
                    type="checkbox"
                    checked={petFriendly}
                    onChange={(e) => setPetFriendly(e.target.checked)}
                    className="w-4 h-4 text-cyan-500 rounded border-theme-border focus:ring-cyan-500 bg-theme-main"
                  />
                  <span className="text-sm font-medium text-theme-text group-hover:text-cyan-400 transition-colors">Pet Friendly</span>
                </label>
              </div>
              <div className="flex flex-col items-center justify-center pt-5">
                <label className="flex items-center space-x-2 cursor-pointer group">
                  <input
                    type="checkbox"
                    checked={nearMetro}
                    onChange={(e) => setNearMetro(e.target.checked)}
                    className="w-4 h-4 text-cyan-500 rounded border-theme-border focus:ring-cyan-500 bg-theme-main"
                  />
                  <span className="text-sm font-medium text-theme-text group-hover:text-cyan-400 transition-colors">Near Metro</span>
                </label>
              </div>
            </div>

            {/* Descriptions */}
            <div className="md:col-span-2 space-y-4">
              <div>
                <label className="block text-sm font-semibold text-theme-text mb-1">Vector DB Description (Primary RAG Payload) <span className="text-red-500">*</span></label>
                <textarea
                  required
                  rows={3}
                  placeholder="Detailed description used for hybrid vector embeddings..."
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-4 py-3 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50 transition-shadow"
                />
              </div>
              <div>
                <label className="block text-sm font-semibold text-theme-text mb-1">Secondary Description (Additional Details)</label>
                <textarea
                  rows={2}
                  placeholder="Additional promotional or logistical details..."
                  value={secondaryDescription}
                  onChange={(e) => setSecondaryDescription(e.target.value)}
                  className="w-full bg-theme-main border border-theme-border text-theme-text rounded-xl px-4 py-3 text-sm focus:outline-none focus:ring-2 focus:ring-cyan-500/50 transition-shadow"
                />
              </div>
            </div>
          </div>

          <div className="flex items-center justify-end space-x-4 pt-4 border-t border-theme-border/50">
            {isEditing && (
              <button
                type="button"
                onClick={resetForm}
                className="px-6 py-2.5 text-sm font-medium text-theme-sub hover:text-theme-text transition-colors"
              >
                Cancel Edit
              </button>
            )}
            <button
              type="submit"
              disabled={addListingMutation.isPending || updateListingMutation.isPending}
              className="bg-white dark:bg-black hover:bg-zinc-100 dark:hover:bg-zinc-900 text-black dark:text-white font-bold text-sm px-8 py-2.5 rounded-xl transition-all shadow-lg active:scale-95 disabled:opacity-50 disabled:cursor-not-allowed border border-slate-300 dark:border-white/20"
            >
              {isEditing 
                ? (updateListingMutation.isPending ? "Updating..." : "Update Property")
                : (addListingMutation.isPending ? "Adding..." : "Add Property to System")}
            </button>
          </div>
        </form>
      </div>

      {/* Listings Management Section */}
      <div className="bg-theme-card border border-theme-border rounded-2xl p-6 shadow-xl">
        <h3 className="text-2xl font-bold text-theme-text font-serif mb-6">Manage Existing Properties</h3>
        
        {isLoadingListings ? (
          <div className="flex justify-center p-8">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-cyan-500"></div>
          </div>
        ) : listingsData?.listings && listingsData.listings.length > 0 ? (
          <div className="overflow-x-auto rounded-xl border border-theme-border">
            <table className="w-full text-left text-sm">
              <thead className="bg-theme-main text-theme-sub">
                <tr>
                  <th className="px-4 py-3 font-semibold">Title</th>
                  <th className="px-4 py-3 font-semibold">Location</th>
                  <th className="px-4 py-3 font-semibold">Price (AED)</th>
                  <th className="px-4 py-3 font-semibold">Type</th>
                  <th className="px-4 py-3 font-semibold text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-theme-border/50">
                {listingsData.listings.map((listing) => (
                  <tr key={listing.id} className="hover:bg-theme-main/50 transition-colors">
                    <td className="px-4 py-3">
                      <div className="font-medium text-theme-text">{listing.title}</div>
                      <div className="text-xs text-theme-sub max-w-[250px] truncate">{listing.description}</div>
                    </td>
                    <td className="px-4 py-3 text-theme-text">{listing.location}</td>
                    <td className="px-4 py-3 text-theme-text">{formatNumber(listing.price_aed ?? listing.price)}</td>
                    <td className="px-4 py-3">
                      <span className={`px-2 py-1 text-xs rounded-md ${listing.listing_type === 'rent' ? 'bg-blue-500/10 text-blue-400' : 'bg-emerald-500/10 text-emerald-400'}`}>
                        {listing.listing_type?.toUpperCase()}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-right space-x-3">
                      <button
                        onClick={() => handleEdit(listing)}
                        className="text-cyan-400 hover:text-cyan-300 text-xs font-medium transition-colors"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => handleDelete(listing.id)}
                        disabled={deleteListingMutation.isPending}
                        className="text-red-500 hover:text-red-400 text-xs font-medium transition-colors disabled:opacity-50"
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="text-center p-8 text-theme-sub bg-theme-main/30 rounded-xl border border-dashed border-theme-border">
            No properties found in the system.
          </div>
        )}
      </div>
    </div>
  );
}
