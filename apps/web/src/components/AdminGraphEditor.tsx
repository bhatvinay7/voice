"use client";

import React, { useState } from "react";
import { useAdminGraphNodes, useAdminAddGraphNode, useAdminUpdateGraphNode, useAdminDeleteGraphNode } from "../lib/api";
import { AdminGraphNode } from "../../types/graph";

export function AdminGraphEditor(): React.ReactElement {
  const { data, isLoading } = useAdminGraphNodes();
  const addNodeMutation = useAdminAddGraphNode();
  const updateNodeMutation = useAdminUpdateGraphNode();
  const deleteNodeMutation = useAdminDeleteGraphNode();

  const [stepType, setStepType] = useState("DynamicErrorResolver");
  const [instruction, setInstruction] = useState("");
  const [editingNodeId, setEditingNodeId] = useState<string | null>(null);
  const [editInstruction, setEditInstruction] = useState("");

  const handleAddNode = (e: React.FormEvent): void => {
    e.preventDefault();
    if (!instruction) return;

    const newNode: AdminGraphNode = {
      id: `node-${Date.now()}`,
      step_type: stepType,
      instruction,
      status: "Active",
      is_preset: false,
    };

    addNodeMutation.mutate(newNode, {
      onSuccess: () => {
        setInstruction("");
      },
    });
  };

  const startEditing = (node: AdminGraphNode) => {
    setEditingNodeId(node.id);
    setEditInstruction(node.instruction);
  };

  const saveEdit = (id: string) => {
    if (!editInstruction) return;
    updateNodeMutation.mutate({ id, instruction: editInstruction }, {
      onSuccess: () => setEditingNodeId(null)
    });
  };

  return (
    <div className="bg-slate-900/90 border border-slate-800 rounded-2xl p-6 shadow-2xl space-y-6">
      <div>
        <h3 className="text-xl font-bold text-white">Graph-Based Workflow State Machine Editor</h3>
        <p className="text-xs text-slate-400">
          Memgraph / Neo4j static DAG instruction workflow editor for AI agents.
        </p>
      </div>

      {/* Add New DAG Node Form */}
      <form onSubmit={handleAddNode} className="grid grid-cols-1 md:grid-cols-3 gap-4 bg-slate-800/60 p-4 rounded-xl border border-slate-700/60">
        <div>
          <label className="block text-xs font-semibold text-slate-300 mb-1">Step Type</label>
          <select
            value={stepType}
            onChange={(e) => setStepType(e.target.value)}
            className="w-full bg-slate-900 border border-slate-700 text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-purple-500"
          >
            <option value="UserInputVerifier">UserInputVerifier</option>
            <option value="RagFetchAndAgentAnalysis">RagFetchAndAgentAnalysis</option>
            <option value="SlotBookingAndOutbox">SlotBookingAndOutbox</option>
            <option value="DynamicErrorResolver">DynamicErrorResolver</option>
          </select>
        </div>
        <div className="md:col-span-2 flex items-end space-x-3">
          <div className="flex-1">
            <label className="block text-xs font-semibold text-slate-300 mb-1">Instruction Text</label>
            <input
              type="text"
              placeholder="e.g. Run dynamic compilation & error resolution node..."
              value={instruction}
              onChange={(e) => setInstruction(e.target.value)}
              className="w-full bg-slate-900 border border-slate-700 text-white rounded-lg px-3 py-2 text-xs focus:outline-none focus:border-purple-500"
            />
          </div>
          <button
            type="submit"
            disabled={addNodeMutation.isPending}
            className="bg-purple-600 hover:bg-purple-500 text-white px-5 py-2 rounded-lg text-xs font-bold transition shadow-md shadow-purple-600/30"
          >
            {addNodeMutation.isPending ? "Adding..." : "Add DAG Node"}
          </button>
        </div>
      </form>

      {/* DAG Workflow Graph Nodes List */}
      {isLoading ? (
        <p className="text-xs text-slate-400">Loading graph nodes...</p>
      ) : (
        <div className="space-y-4">
          <p className="text-xs font-bold text-slate-300 uppercase tracking-wider">Active DAG Nodes</p>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            {data?.graph_nodes.map((node, index) => (
              <div
                key={node.id}
                className="bg-slate-950 p-4 rounded-xl border border-purple-900/40 relative space-y-2 shadow-lg group"
              >
                <div className="flex justify-between items-center">
                  <span className="text-[10px] font-mono font-bold bg-purple-950 text-purple-300 px-2 py-0.5 rounded border border-purple-800">
                    Step {index + 1}: {node.step_type}
                  </span>
                  <div className="flex items-center gap-2">
                    <span className="text-[10px] text-emerald-400 font-semibold">{node.status}</span>
                    {!node.is_preset && (
                      <button onClick={() => deleteNodeMutation.mutate(node.id)} className="opacity-0 group-hover:opacity-100 text-red-500 hover:text-red-400 transition-opacity">
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"></path></svg>
                      </button>
                    )}
                  </div>
                </div>
                
                {editingNodeId === node.id ? (
                  <div className="mt-2 flex gap-2">
                    <input 
                      type="text" 
                      value={editInstruction} 
                      onChange={(e) => setEditInstruction(e.target.value)} 
                      className="w-full bg-slate-900 border border-slate-700 text-white rounded px-2 py-1 text-xs focus:outline-none focus:border-purple-500"
                    />
                    <button onClick={() => saveEdit(node.id)} className="text-xs bg-emerald-600 hover:bg-emerald-500 px-2 rounded font-bold text-white">Save</button>
                    <button onClick={() => setEditingNodeId(null)} className="text-xs bg-slate-700 hover:bg-slate-600 px-2 rounded font-bold text-white">Cancel</button>
                  </div>
                ) : (
                  <p 
                    className="text-xs text-slate-200 font-medium cursor-pointer hover:bg-slate-900 p-1 -mx-1 rounded"
                    onClick={() => startEditing(node)}
                  >
                    {node.instruction}
                  </p>
                )}
                
                <span className="text-[10px] text-slate-500 block">
                  {node.is_preset ? "Preset Workflow Step (Read-Only Type)" : "Dynamic Resolution Step"}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
