"use client";

import { useSearchParams, useRouter } from "next/navigation";
import { useEffect, Suspense } from "react";

function RoundsRedirect() {
  const searchParams = useSearchParams();
  const router = useRouter();
  const roundId = searchParams.get("round");
  
  useEffect(() => {
    // Redirect to home page with the round parameter
    if (roundId) {
      router.replace(`/?round=${roundId}`);
    } else {
      router.replace("/");
    }
  }, [roundId, router]);
  
  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 flex items-center justify-center">
      <div className="flex items-center gap-3 text-slate-400">
        <div className="w-6 h-6 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
        <span>Redirecting to round {roundId}...</span>
      </div>
    </div>
  );
}

export default function RoundsPage() {
  return (
    <Suspense fallback={
      <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 flex items-center justify-center">
        <div className="w-6 h-6 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
      </div>
    }>
      <RoundsRedirect />
    </Suspense>
  );
}

