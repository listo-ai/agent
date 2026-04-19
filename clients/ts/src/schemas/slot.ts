import { z } from "zod";

export const WriteSlotResponseSchema = z.object({
  generation: z.number().int().nonnegative(),
});

export type WriteSlotResponse = z.infer<typeof WriteSlotResponseSchema>;
