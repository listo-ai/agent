/**
 * Shared React contexts for the flow canvas components.
 *
 * Extracted so FlowNodeCard.tsx can consume LiveDataContext without
 * importing the entire FlowCanvas module (avoids a circular dependency).
 */
import { createContext } from "react";
import type { LiveNodeState } from "../flow-model";

export const LiveDataContext = createContext<Record<string, LiveNodeState>>({});
