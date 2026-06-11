export type VenueErrorCode =
  | 'auth'
  | 'insufficient_funds'
  | 'bad_symbol'
  | 'rate_limited'
  | 'venue_unavailable'
  | 'invalid_request'
  | 'unknown';

export class VenueError extends Error {
  constructor(
    public readonly venueId: string,
    public readonly code: VenueErrorCode,
    message: string,
    public readonly httpStatus?: number,
    public readonly venueCode?: string | number,
  ) {
    super(`[${venueId}] ${code}: ${message}`);
    this.name = 'VenueError';
  }
}
