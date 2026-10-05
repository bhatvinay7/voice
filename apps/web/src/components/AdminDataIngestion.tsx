"use client";

import React, { useState } from "react";
import { useAdminAddListing } from "../lib/api";
import { PropertyListing } from "../../types/listing";

export function AdminDataIngestion(): React.ReactElement {
  const addListingMutation = useAdminAddListing();

  const [title, setTitle] = useState<string>("");
  const [location, setLocation] = useState<string>("Dubai Marina");
  const [priceInput, setPriceInput] = useState<number>(110000);
  const [bedrooms, setBedrooms] = useState<number>(2);
  const [bathrooms, setBathrooms] = useState<number>(2);
  const [areaSqft, setAreaSqft] = useState<number>(1200);
  const [description, setDescription] = useState<string>("");

  const handleSubmit = (e: React.FormEvent): void => {
    e.preventDefault();
    if (!title || !description) return;

    const newListing: PropertyListing = {
      id: `prop-${Date.now()}`,
      title,
      secondary_title: title,
      description,
      secondary_description: description,
      city: "Dubai",
      location,
      price: Number(priceInput),
      listing_type: "rent",
      bedrooms: Number(bedrooms),
      bathrooms: Number(bathrooms),
      area_sqft: Number(areaSqft),
      pet_friendly: true,
      near_metro: true,
      agent_id: "agent-001",
      available_slots: ["2026-10-05T10:00:00Z"],
    };

    addListingMutation.mutate(newListing, {
      onSuccess: () => {
        setTitle("");
        setDescription("");
      },
    });
  };

  return (
    <div className="bg-slate-900/90 border border-slate-800 rounded-2xl p-6 shadow-2xl space-y-6">
      <div>
        <h3 className="text-xl font-bold text-white">PostgreSQL & Vector DB Data Ingestion</h3>
        <p className="text-xs text-slate-400">
          Ingest new real estate listings into PostgreSQL relational store and hybrid Vector DB RAG embeddings index.
        </p>
      </div>

      <form onSubmit={handleSubmit} className="space-y-4 bg-slate-800/60 p-5 rounded-xl border border-slate-700/60">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label className="block text-xs font-semibold text-slate-300 mb-1">Listing Title</label>
            <input
              type="text"
              placeholder="e.g. Luxury 2BR Executive Suite with Sea View"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="w-full bg-slate-900 border border-slate-700 text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-blue-500"
            />
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-300 mb-1">Location / District</label>
            <select
              value={location}
              onChange={(e) => setLocation(e.target.value)}
              className="w-full bg-slate-900 border border-slate-700 text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-blue-500"
            >
              <option value="Dubai Marina">Dubai Marina</option>
              <option value="Downtown Dubai">Downtown Dubai</option>
              <option value="Palm Jumeirah">Palm Jumeirah</option>
              <option value="Business Bay">Business Bay</option>
              <option value="Jumeirah Village Circle">Jumeirah Village Circle</option>
            </select>
          </div>

          <div>
            <label className="block text-xs font-semibold text-slate-300 mb-1">Annual Price ($)</label>
            <input
              type="number"
              value={priceInput}
              onChange={(e) => setPriceInput(Number(e.target.value))}
              className="w-full bg-slate-900 border border-slate-700 text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-blue-500"
            />
          </div>

          <div className="grid grid-cols-3 gap-2">
            <div>
              <label className="block text-xs font-semibold text-slate-300 mb-1">Bedrooms</label>
              <input
                type="number"
                value={bedrooms}
                onChange={(e) => setBedrooms(Number(e.target.value))}
                className="w-full bg-slate-900 border border-slate-700 text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-slate-300 mb-1">Bathrooms</label>
              <input
                type="number"
                value={bathrooms}
                onChange={(e) => setBathrooms(Number(e.target.value))}
                className="w-full bg-slate-900 border border-slate-700 text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-blue-500"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-slate-300 mb-1">Area (sqft)</label>
              <input
                type="number"
                value={areaSqft}
                onChange={(e) => setAreaSqft(Number(e.target.value))}
                className="w-full bg-slate-900 border border-slate-700 text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-blue-500"
              />
            </div>
          </div>
        </div>

        <div>
          <label className="block text-xs font-semibold text-slate-300 mb-1">Vector DB Description (RAG Embedding Payload)</label>
          <textarea
            rows={3}
            placeholder="Detailed description used for hybrid vector embeddings (e.g. Modern high-floor 2BR apartment near Sobha Realty Metro station...)"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            className="w-full bg-slate-900 border border-slate-700 text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-blue-500"
          />
        </div>

        <button
          type="submit"
          disabled={addListingMutation.isPending}
          className="bg-emerald-600 hover:bg-emerald-500 text-white font-bold text-xs px-6 py-2.5 rounded-xl transition shadow-lg shadow-emerald-600/30"
        >
          {addListingMutation.isPending ? "Indexing into DB..." : "Add to Postgres & Vector DB"}
        </button>
      </form>
    </div>
  );
};
