import { useEffect, useMemo, useState } from "react";
import type { AgentClient, NodeSnapshot } from "@sys/agent-client";
import { queryClient } from "@/providers/query";
import { slotMap, type LiveNodeState } from "./flow-model";

type LiveNodeMap = Record<string, LiveNodeState>;

export function useFlowLiveData(agent: AgentClient | undefined, seedNodes: NodeSnapshot[]) {
  const initialState = useMemo<LiveNodeMap>(
    () =>
      Object.fromEntries(
        seedNodes.map((node) => [
          node.path,
          {
            lifecycle: node.lifecycle as LiveNodeState["lifecycle"],
            slots: slotMap(node.slots),
            touchedAt: Date.now(),
          },
        ]),
      ),
    [seedNodes],
  );

  const [liveByPath, setLiveByPath] = useState<LiveNodeMap>(initialState);

  useEffect(() => {
    setLiveByPath(initialState);
  }, [initialState]);

  useEffect(() => {
    if (!agent) {
      return undefined;
    }

    const sub = agent.events.subscribe();
    let cancelled = false;

    const run = async () => {
      for await (const event of sub) {
        if (cancelled) {
          break;
        }

        switch (event.event) {
          case "slot_changed":
            setLiveByPath((current) => {
              const prev = current[event.path] ?? {
                lifecycle: undefined,
                slots: undefined,
                touchedAt: undefined,
              };
              const prevSlots = prev.slots ?? {};
              return {
                ...current,
                [event.path]: {
                  ...prev,
                  slots: {
                    ...prevSlots,
                    [event.slot]: {
                      name: event.slot,
                      value: event.value,
                      generation: event.generation,
                    },
                },
                  touchedAt: Date.now(),
                },
              };
            });
            break;

          case "lifecycle_transition":
            setLiveByPath((current) => ({
              ...current,
              [event.path]: {
                ...(current[event.path] ?? {
                  lifecycle: undefined,
                  slots: undefined,
                  touchedAt: undefined,
                }),
                lifecycle: event.to,
                touchedAt: Date.now(),
              },
            }));
            break;

          case "node_created":
          case "node_removed":
          case "node_renamed":
            void queryClient.invalidateQueries({ queryKey: ["nodes"] });
            break;

          case "link_added":
          case "link_removed":
          case "link_broken":
            void queryClient.invalidateQueries({ queryKey: ["links"] });
            break;
        }
      }
    };

    void run();

    return () => {
      cancelled = true;
      sub.close();
    };
  }, [agent]);

  return liveByPath;
}
