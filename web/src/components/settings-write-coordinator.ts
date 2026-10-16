import type { ConfigClient, ConfigEntry, ConfigWrite, WriteResult } from "@/api/config";

type Flight = { pending: number; revision: number; overlapping: boolean };

/** Whole-entry responses cannot be ordered by the order requests were sent. */
export class SettingsWriteCoordinator {
  private readonly flights = new Map<string, Flight>();

  async write(
    client: ConfigClient,
    csrf: string,
    request: ConfigWrite,
    apply: (entry: ConfigEntry) => void,
  ): Promise<WriteResult> {
    const flight = this.flights.get(request.name) ?? {
      pending: 0,
      revision: 0,
      overlapping: false,
    };
    this.flights.set(request.name, flight);
    flight.overlapping ||= flight.pending > 0;
    flight.pending += 1;
    flight.revision += 1;
    let result: WriteResult;
    try {
      result = await client.write(csrf, request);
    } catch (error) {
      result = { ok: false, status: null, message: String(error) };
    }
    flight.pending -= 1;
    if (!flight.overlapping) {
      if (result.ok) apply(result.entry);
      return result;
    }
    if (flight.pending > 0) return result;
    const revision = flight.revision;
    try {
      // Every overlapping mutation has settled, so this read observes their
      // persisted result regardless of application or response delivery order.
      const snapshot = await client.load();
      const entry = snapshot.entries.find((item) => item.name === request.name);
      if (!entry) throw new Error(`config response missing ${request.name}`);
      if (revision === flight.revision) {
        flight.overlapping = false;
        apply(entry);
        if (result.ok) return { ok: true, entry };
      }
    } catch (error) {
      return {
        ok: false,
        status: null,
        message: `Could not refresh saved settings: ${String(error)}`,
      };
    }
    return result;
  }
}
