"use client";

import React, { JSX } from "react";
import { useContractReviewQueue, useApproveContract } from "../lib/api";

export function AdminContractReview(): React.ReactElement {
  const { data, isLoading } = useContractReviewQueue();
  const approveMutation = useApproveContract();

  const handleApprove = (contractId: string): void => {
    approveMutation.mutate(contractId);
  };

  return (
    <div className="bg-slate-900/90 border border-slate-800 rounded-2xl p-6 shadow-2xl space-y-6">
      <div className="flex justify-between items-center">
        <div>
          <h3 className="text-xl font-bold text-white">Tenancy Contract Human Review Queue</h3>
          <p className="text-xs text-slate-400">
            Low-confidence extracted contracts (&lt; 0.85) pending human verification
          </p>
        </div>
        <span className="text-xs bg-amber-950 text-amber-300 px-3 py-1 rounded-full border border-amber-800 font-semibold">
          Pending Approvals: {data?.review_queue.length || 0}
        </span>
      </div>

      {isLoading ? (
        <p className="text-xs text-slate-400">Loading review queue...</p>
      ) : data?.review_queue.length === 0 ? (
        <div className="p-8 text-center bg-slate-950/60 rounded-xl border border-slate-800 text-slate-400 text-xs font-medium">
          ✅ No low-confidence tenancy contracts in review queue. All contracts approved!
        </div>
      ) : (
        <div className="space-y-4">
          {data?.review_queue.map((contract) => (
            <div
              key={contract.id}
              className="bg-slate-950 p-5 rounded-xl border border-amber-900/50 space-y-3 shadow-lg"
            >
              <div className="flex justify-between items-start">
                <div>
                  <h4 className="font-bold text-white text-sm">{contract.filename}</h4>
                  <p className="text-xs text-slate-400">{contract.property_address}</p>
                </div>
                <span className="text-xs font-mono font-bold text-amber-400 bg-amber-950 px-2.5 py-1 rounded border border-amber-800">
                  Confidence Score: {(contract.confidence_score * 100).toFixed(0)}%
                </span>
              </div>

              <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-xs bg-slate-900/80 p-3 rounded-lg border border-slate-800">
                <div>
                  <span className="text-slate-400 block text-[10px] uppercase">Landlord</span>
                  <span className="text-white font-medium">{contract.landlord_name}</span>
                </div>
                <div>
                  <span className="text-slate-400 block text-[10px] uppercase">Tenant</span>
                  <span className="text-white font-medium">{contract.tenant_name}</span>
                </div>
                <div>
                  <span className="text-slate-400 block text-[10px] uppercase">Annual Rent</span>
                  <span className="text-emerald-400 font-bold">${contract.annual_rent.toLocaleString()}</span>
                </div>
                <div>
                  <span className="text-slate-400 block text-[10px] uppercase">Notice Period</span>
                  <span className="text-white font-medium">{contract.notice_period_days} Days</span>
                </div>
              </div>

              <div className="p-3 bg-slate-900/50 rounded-lg text-xs text-slate-300">
                <span className="font-semibold text-amber-300 block mb-1">Clause Summary:</span>
                {contract.clause_summary}
              </div>

              <div className="flex justify-between items-center pt-2">
                <span className="text-[10px] font-mono text-slate-500">R2 Storage: {contract.r2_vault_path}</span>
                <button
                  onClick={() => handleApprove(contract.id)}
                  disabled={approveMutation.isPending}
                  className="bg-emerald-600 hover:bg-emerald-500 text-white font-bold text-xs px-4 py-2 rounded-lg transition shadow-md shadow-emerald-600/30"
                >
                  {approveMutation.isPending ? "Approving..." : "Approve Extracted Fields"}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
