"use client";

import React, { useState } from "react";
import { PropertyListing } from "../../types/listing";
import { useBookViewingSlot } from "../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import { Search, MapPin, Home, Calendar, ChevronRight, Sparkles } from "lucide-react";
import { formatNumber } from "../lib/formatters";

interface PropertySearchProps {
  listings: PropertyListing[];
  userId: string;
  isVerified: boolean;
  agentModeActive: boolean;
  isLoading?: boolean;
}

export function PropertySearch({
  listings,
  userId,
  isVerified,
  agentModeActive,
  isLoading = false,
}: PropertySearchProps): React.ReactElement {
  const [searchTerm, setSearchTerm] = useState("");
  const [selectedCity, setSelectedCity] = useState<string>("All");
  const [selectedType, setSelectedType] = useState<string>("All");
  const [bookingMessage, setBookingMessage] = useState<{ text: string; isError: boolean } | null>(null);

  const bookSlotMutation = useBookViewingSlot();

  const filteredListings = listings.filter((listing) => {
    const matchesSearch =
      listing.title.toLowerCase().includes(searchTerm.toLowerCase()) ||
      listing.location.toLowerCase().includes(searchTerm.toLowerCase());
    const matchesCity = selectedCity === "All" || listing.city === selectedCity;
    const matchesType = selectedType === "All" || listing.listing_type === selectedType;
    return matchesSearch && matchesCity && matchesType;
  });

  const handleBookSlot = (listingId: string, slotTime: string) => {
    const idempotencyKey = `idemp-web-${Date.now()}`;
    bookSlotMutation.mutate(
      {
        listingId,
        userId,
        slotTime,
        idempotencyKey,
      },
      {
        onSuccess: (data) => {
          if (!data.success) {
            setBookingMessage({
              text: data.error || "Booking failed.",
              isError: true,
            });
          } else {
            setBookingMessage({
              text: `Viewing slot successfully reserved for ${new Date(slotTime).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}!`,
              isError: false,
            });
          }
        },
        onError: (err) => {
          setBookingMessage({
            text: `Booking error: ${err.message}`,
            isError: true,
          });
        },
      }
    );
  };

  return (
    <div className="space-y-8">
      {/* Banner */}
      <motion.div 
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        className="relative overflow-hidden rounded-[2rem] p-8 md:p-10 border border-theme-border dark:border-white/15 bg-theme-main group transition-colors duration-300 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]"
      >
        <div className="relative z-10 flex flex-col md:flex-row md:items-end justify-between gap-6">
          <div className="space-y-4 max-w-2xl">
            <div className="inline-flex items-center space-x-2 bg-theme-main border border-theme-border dark:border-white/20 px-3 py-1 rounded-full shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)]">
              <Sparkles className="w-3.5 h-3.5 text-cyan-400" />
              <span className="text-xs font-bold tracking-widest text-theme-text uppercase">
                {agentModeActive ? "AI Autonomous Mode ACTIVE" : "Manual Search Mode"}
              </span>
            </div>
            <h2 className="text-3xl md:text-4xl font-extrabold text-theme-text tracking-tight font-serif">
              Premium Properties
            </h2>
            <p className="text-theme-sub text-sm md:text-base leading-relaxed">
              Browse verified listings globally. {agentModeActive ? "Our AI assistant is active to handle voice queries, natural language filtering, and instant slot reservations." : "Use manual controls to filter properties."}
            </p>
          </div>
        </div>
      </motion.div>

      {/* Booking Status Message Banner */}
      <AnimatePresence>
        {bookingMessage && (
          <motion.div
            initial={{ opacity: 0, y: -10, height: 0 }}
            animate={{ opacity: 1, y: 0, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            className={`p-4 rounded-xl text-sm font-medium border flex items-center shadow-lg dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)] ${
              bookingMessage.isError
                ? "bg-rose-500/10 text-rose-300 border-rose-500/30"
                : "bg-emerald-500/10 text-emerald-300 border-emerald-500/30"
            }`}
          >
            <div className={`w-2 h-2 rounded-full mr-3 ${bookingMessage.isError ? "bg-rose-500" : "bg-emerald-500"}`} />
            {bookingMessage.text}
          </motion.div>
        )}
      </AnimatePresence>

      {/* Manual Search Controls */}
      <motion.div 
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
        className="grid grid-cols-1 md:grid-cols-12 gap-4 bg-theme-main p-3 rounded-2xl border border-theme-border dark:border-white/15 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]"
      >
        <div className="md:col-span-6 relative group">
          <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-theme-sub group-focus-within:text-theme-text transition-colors" />
          <input
            type="text"
            placeholder="Search location or title (e.g. Downtown, Westside)..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="w-full bg-theme-card border border-theme-border dark:border-white/15 text-theme-text rounded-xl pl-10 pr-4 py-3 text-sm focus:outline-none focus:border-theme-accent transition-all placeholder:text-theme-sub dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]"
          />
        </div>
        <div className="md:col-span-3 relative group">
          <MapPin className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-theme-sub group-focus-within:text-theme-text transition-colors" />
          <select
            value={selectedCity}
            onChange={(e) => setSelectedCity(e.target.value)}
            className="w-full appearance-none bg-theme-card border border-theme-border dark:border-white/15 text-theme-text rounded-xl pl-10 pr-4 py-3 text-sm focus:outline-none focus:border-theme-accent transition-all dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]"
          >
            <option value="All">All Regions</option>
            <option value="Metropolis">Metropolis</option>
            <option value="Downtown">Downtown</option>
            <option value="Suburbs">Suburbs</option>
          </select>
        </div>
        <div className="md:col-span-3 relative group">
          <Home className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-theme-sub group-focus-within:text-theme-text transition-colors" />
          <select
            value={selectedType}
            onChange={(e) => setSelectedType(e.target.value)}
            className="w-full appearance-none bg-theme-card border border-theme-border dark:border-white/15 text-theme-text rounded-xl pl-10 pr-4 py-3 text-sm focus:outline-none focus:border-theme-accent transition-all dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]"
          >
            <option value="All">Rent & Buy</option>
            <option value="rent">For Rent</option>
            <option value="buy">For Sale</option>
          </select>
        </div>
      </motion.div>

      {/* Listing Cards */}
      <motion.div 
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.2 }}
        className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6"
      >
        {isLoading ? (
          // Loading Skeletons
          Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="bg-theme-card border border-theme-border dark:border-white/15 rounded-3xl overflow-hidden p-6 flex flex-col justify-between h-[340px] dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
              <div className="space-y-4">
                <div className="flex justify-between items-start">
                  <div className="w-20 h-6 bg-theme-main rounded-full animate-pulse"></div>
                  <div className="w-24 h-6 bg-theme-main rounded-lg animate-pulse"></div>
                </div>
                <div className="space-y-2 pt-2">
                  <div className="w-3/4 h-6 bg-theme-main rounded-lg animate-pulse"></div>
                  <div className="w-1/2 h-4 bg-theme-main rounded-lg animate-pulse"></div>
                  <div className="w-full h-10 bg-theme-main rounded-lg animate-pulse mt-4"></div>
                </div>
              </div>
              <div className="w-full h-10 bg-theme-main rounded-xl animate-pulse mt-6"></div>
            </div>
          ))
        ) : (
          filteredListings.map((listing, i) => (
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.1 * i }}
            key={listing.id}
            className="group relative bg-theme-card border border-theme-border dark:border-white/15 rounded-[2rem] overflow-hidden hover:border-white/40 transition-all duration-300 flex flex-col justify-between shadow-xl dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]"
          >
            <div className="p-7 space-y-5">
              <div className="flex justify-between items-start">
                <span className="text-[10px] font-bold px-3 py-1.5 rounded-full bg-theme-main text-theme-sub border border-theme-border dark:border-white/10 uppercase tracking-wider group-hover:text-theme-text transition-colors font-mono dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]">
                  {listing.listing_type === "rent" ? "For Rent" : "For Sale"}
                </span>
                <span className="text-xl font-extrabold text-emerald-400 font-mono">
                  AED {formatNumber(listing.price_aed ?? listing.price)} <span className="text-xs font-bold text-theme-sub">{listing.listing_type === "rent" ? "/yr" : ""}</span>
                </span>
              </div>

              <div className="space-y-2">
                <h3 className="font-bold text-theme-text text-xl leading-snug tracking-tight font-sans">{listing.title}</h3>
                <p className="text-xs text-theme-sub flex items-center gap-1.5 font-medium">
                  <MapPin className="w-3.5 h-3.5 text-cyan-400" />
                  {listing.location}, {listing.city}
                </p>
                <p className="text-xs text-theme-sub leading-relaxed line-clamp-2 pt-1 font-normal">{listing.description}</p>
              </div>

              <div className="flex items-center justify-between text-xs font-medium text-theme-text pt-4 border-t border-theme-border dark:border-white/10">
                <div className="flex flex-col items-center justify-center p-2.5 rounded-2xl bg-theme-main border border-theme-border dark:border-white/10 w-full mr-2 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]">
                  <span className="text-theme-sub mb-0.5 text-[10px] uppercase tracking-wider font-mono">Beds</span>
                  <span className="text-theme-text font-bold text-sm">{listing.bedrooms}</span>
                </div>
                <div className="flex flex-col items-center justify-center p-2.5 rounded-2xl bg-theme-main border border-theme-border dark:border-white/10 w-full mr-2 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]">
                  <span className="text-theme-sub mb-0.5 text-[10px] uppercase tracking-wider font-mono">Baths</span>
                  <span className="text-theme-text font-bold text-sm">{listing.bathrooms}</span>
                </div>
                <div className="flex flex-col items-center justify-center p-2.5 rounded-2xl bg-theme-main border border-theme-border dark:border-white/10 w-full dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]">
                  <span className="text-theme-sub mb-0.5 text-[10px] uppercase tracking-wider font-mono">Sqft</span>
                  <span className="text-theme-text font-bold text-sm">{listing.area_sqft}</span>
                </div>
              </div>
            </div>

            <div className="p-5 bg-theme-main border-t border-theme-border dark:border-white/10">
              <div className="flex items-center gap-2 mb-3">
                <Calendar className="w-3.5 h-3.5 text-cyan-400" />
                <p className="text-[10px] font-bold text-theme-sub uppercase tracking-wider font-mono">Reserve Viewing Slot</p>
              </div>
              <div className="flex flex-wrap gap-2">
                {listing.available_slots.map((slot) => (
                  <button
                    key={slot}
                    onClick={() => handleBookSlot(listing.id, slot)}
                    className="text-xs bg-white dark:bg-black hover:bg-zinc-100 dark:hover:bg-zinc-900 text-black dark:text-white font-bold px-4 py-2 rounded-xl border border-slate-300 dark:border-white/20 transition-all overflow-hidden flex-1 text-center font-mono shadow-sm"
                  >
                    <span className="relative z-10 flex items-center justify-center gap-1">
                      {new Date(slot).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                    </span>
                  </button>
                ))}
                {listing.available_slots.length === 0 && (
                  <p className="text-xs text-theme-sub italic font-mono">No slots available</p>
                )}
              </div>
            </div>
          </motion.div>
        )))}
        {!isLoading && filteredListings.length === 0 && (
          <div className="col-span-full py-20 flex flex-col items-center justify-center text-theme-sub border border-dashed border-theme-border rounded-3xl">
            <Search className="w-10 h-10 mb-4 opacity-50" />
            <p className="text-lg font-medium text-theme-text">No properties found</p>
            <p className="text-sm mt-1 text-theme-sub">Try adjusting your filters</p>
          </div>
        )}
      </motion.div>
    </div>
  );
};
