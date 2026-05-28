import { QueryClient } from '@tanstack/react-query';

type ErrorWithStatus = { status?: number };

function shouldRetry(failureCount: number, error: unknown): boolean {
  const status = (error as ErrorWithStatus | null)?.status;
  if (typeof status === 'number' && status >= 400 && status < 500) {
    return false;
  }
  return failureCount < 3;
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 30_000,
        gcTime: 5 * 60_000,
        refetchOnWindowFocus: false,
        retry: shouldRetry,
      },
    },
  });
}
