"use client";

import React, { useState } from "react";
import { PropertyListing } from "../../types/listing";
import { useBookViewingSlot } from "../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import { Search, MapPin, Home, Calendar, ChevronRight, Sparkles } from "lucide-react";

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
        className="relative overflow-hidden rounded-[2rem] p-8 md:p-10 border border-slate-700/50 bg-gradient-to-br from-slate-900/80 via-slate-900/90 to-[#0c142e] shadow-2xl group"
      >
        {/* Glow Effects */}
        <div className="absolute top-0 right-0 -mt-20 -mr-20 w-64 h-64 bg-cyan-500/10 rounded-full blur-3xl pointer-events-none group-hover:bg-cyan-500/20 transition-all duration-700" />
        <div className="absolute bottom-0 left-0 -mb-20 -ml-20 w-64 h-64 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none group-hover:bg-indigo-500/20 transition-all duration-700" />
        
        <div className="relative z-10 flex flex-col md:flex-row md:items-end justify-between gap-6">
          <div className="space-y-4 max-w-2xl">
            <div className="inline-flex items-center space-x-2 bg-indigo-500/10 border border-indigo-500/20 px-3 py-1 rounded-full">
              <Sparkles className="w-3.5 h-3.5 text-indigo-400" />
              <span className="text-xs font-bold tracking-widest text-indigo-300 uppercase">
                {agentModeActive ? "AI Autonomous Mode ACTIVE" : "Manual Search Mode"}
              </span>
            </div>
            <h2 className="text-3xl md:text-4xl font-extrabold text-white tracking-tight">
              Premium Properties
            </h2>
            <p className="text-slate-400 text-sm md:text-base leading-relaxed">
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
            className={`p-4 rounded-xl text-sm font-medium border flex items-center shadow-lg ${
              bookingMessage.isError
                ? "bg-rose-500/10 text-rose-200 border-rose-500/20"
                : "bg-emerald-500/10 text-emerald-200 border-emerald-500/20"
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
        className="grid grid-cols-1 md:grid-cols-12 gap-4 bg-slate-900/50 backdrop-blur-xl p-3 rounded-2xl border border-slate-700/50 shadow-xl"
      >
        <div className="md:col-span-6 relative group">
          <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 group-focus-within:text-cyan-400 transition-colors" />
          <input
            type="text"
            placeholder="Search location or title (e.g. Downtown, Westside)..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="w-full bg-slate-950/50 border border-slate-700/50 text-white rounded-xl pl-10 pr-4 py-3 text-sm focus:outline-none focus:border-cyan-500/50 focus:ring-1 focus:ring-cyan-500/50 transition-all shadow-inner"
          />
        </div>
        <div className="md:col-span-3 relative group">
          <MapPin className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 group-focus-within:text-cyan-400 transition-colors" />
          <select
            value={selectedCity}
            onChange={(e) => setSelectedCity(e.target.value)}
            className="w-full appearance-none bg-slate-950/50 border border-slate-700/50 text-white rounded-xl pl-10 pr-4 py-3 text-sm focus:outline-none focus:border-cyan-500/50 focus:ring-1 focus:ring-cyan-500/50 transition-all shadow-inner"
          >
            <option value="All">All Regions</option>
            <option value="Metropolis">Metropolis</option>
            <option value="Downtown">Downtown</option>
            <option value="Suburbs">Suburbs</option>
          </select>
        </div>
        <div className="md:col-span-3 relative group">
          <Home className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 group-focus-within:text-cyan-400 transition-colors" />
          <select
            value={selectedType}
            onChange={(e) => setSelectedType(e.target.value)}
            className="w-full appearance-none bg-slate-950/50 border border-slate-700/50 text-white rounded-xl pl-10 pr-4 py-3 text-sm focus:outline-none focus:border-cyan-500/50 focus:ring-1 focus:ring-cyan-500/50 transition-all shadow-inner"
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
            <div key={i} className="bg-[#0a0f1c] border border-slate-800 rounded-3xl overflow-hidden shadow-lg p-6 flex flex-col justify-between h-[340px]">
              <div className="space-y-4">
                <div className="flex justify-between items-start">
                  <div className="w-20 h-6 bg-slate-800 rounded-full animate-pulse"></div>
                  <div className="w-24 h-6 bg-slate-800 rounded-lg animate-pulse"></div>
                </div>
                <div className="space-y-2 pt-2">
                  <div className="w-3/4 h-6 bg-slate-800 rounded-lg animate-pulse"></div>
                  <div className="w-1/2 h-4 bg-slate-800 rounded-lg animate-pulse"></div>
                  <div className="w-full h-10 bg-slate-800 rounded-lg animate-pulse mt-4"></div>
                </div>
              </div>
              <div className="w-full h-10 bg-slate-800 rounded-xl animate-pulse mt-6"></div>
            </div>
          ))
        ) : (
          filteredListings.map((listing, i) => (
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.1 * i }}
            key={listing.id}
            className="group relative bg-[#0a0f1c] border border-slate-800 rounded-3xl overflow-hidden hover:border-slate-600 transition-all duration-500 shadow-lg hover:shadow-2xl hover:shadow-cyan-500/10 flex flex-col justify-between"
          >
            <div className="absolute inset-0 bg-gradient-to-br from-white/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500 pointer-events-none" />
            
            <div className="p-6 space-y-4">
              <div className="flex justify-between items-start">
                <span className="text-[10px] font-bold px-3 py-1.5 rounded-full bg-slate-800 text-slate-300 border border-slate-700 uppercase tracking-wider group-hover:bg-cyan-500/10 group-hover:text-cyan-400 group-hover:border-cyan-500/20 transition-colors">
                  {listing.listing_type === "rent" ? "For Rent" : "For Sale"}
                </span>
                <span className="text-xl font-extrabold text-transparent bg-clip-text bg-gradient-to-r from-emerald-400 to-cyan-400">
                  ${listing.price.toLocaleString()} <span className="text-sm font-bold text-slate-500">{listing.listing_type === "rent" ? "/yr" : ""}</span>
                </span>
              </div>

              <div className="space-y-2">
                <h3 className="font-bold text-white text-lg leading-snug tracking-tight group-hover:text-cyan-50 transition-colors">{listing.title}</h3>
                <p className="text-xs text-slate-400 flex items-center gap-1.5">
                  <MapPin className="w-3.5 h-3.5 text-slate-500" />
                  {listing.location}, {listing.city}
                </p>
                <p className="text-xs text-slate-400/80 leading-relaxed line-clamp-2 pt-2">{listing.description}</p>
              </div>

              <div className="flex items-center justify-between text-xs font-medium text-slate-300 pt-4 border-t border-slate-800/80">
                <div className="flex flex-col items-center justify-center p-2 rounded-xl bg-slate-900/50 w-full mr-2">
                  <span className="text-slate-500 mb-1 text-[10px] uppercase tracking-wider">Beds</span>
                  <span className="text-white">{listing.bedrooms}</span>
                </div>
                <div className="flex flex-col items-center justify-center p-2 rounded-xl bg-slate-900/50 w-full mr-2">
                  <span className="text-slate-500 mb-1 text-[10px] uppercase tracking-wider">Baths</span>
                  <span className="text-white">{listing.bathrooms}</span>
                </div>
                <div className="flex flex-col items-center justify-center p-2 rounded-xl bg-slate-900/50 w-full">
                  <span className="text-slate-500 mb-1 text-[10px] uppercase tracking-wider">Sqft</span>
                  <span className="text-white">{listing.area_sqft}</span>
                </div>
              </div>
            </div>

            <div className="p-5 bg-slate-900/40 border-t border-slate-800/60 backdrop-blur-sm">
              <div className="flex items-center gap-2 mb-3">
                <Calendar className="w-3.5 h-3.5 text-cyan-500" />
                <p className="text-[10px] font-bold text-slate-400 uppercase tracking-wider">Book Viewing</p>
              </div>
              <div className="flex flex-wrap gap-2">
                {listing.available_slots.map((slot) => (
                  <button
                    key={slot}
                    onClick={() => handleBookSlot(listing.id, slot)}
                    className="group/btn relative text-xs bg-slate-800 hover:bg-cyan-600 text-slate-300 hover:text-white font-medium px-4 py-2 rounded-xl border border-slate-700 hover:border-cyan-500 transition-all overflow-hidden flex-1 text-center"
                  >
                    <span className="relative z-10 flex items-center justify-center gap-1">
                      {new Date(slot).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                    </span>
                  </button>
                ))}
                {listing.available_slots.length === 0 && (
                  <p className="text-xs text-slate-500 italic">No slots available</p>
                )}
              </div>
            </div>
          </motion.div>
        )))}
        {!isLoading && filteredListings.length === 0 && (
          <div className="col-span-full py-20 flex flex-col items-center justify-center text-slate-500 border border-dashed border-slate-800 rounded-3xl">
            <Search className="w-10 h-10 mb-4 opacity-50" />
            <p className="text-lg font-medium text-slate-400">No properties found</p>
            <p className="text-sm mt-1 text-slate-600">Try adjusting your filters</p>
          </div>
        )}
      </motion.div>
    </div>
  );
};
