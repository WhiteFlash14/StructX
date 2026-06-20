// Skeleton for the StrategyWorkbench preview column while a compile is in
// flight. Layout mirrors the real PreviewCards + PayoffShape + LegsBlock +
// PayoffBlock so the swap from skeleton to data doesn't reflow. The
// wb-card class gives each block the existing surface + enter animation.

"use client";

import { Skeleton } from "@/components/ui/Skeleton";

export function WorkbenchPreviewSkeleton() {
  return (
    <>
      {/* Preview stats card */}
      <div className="wb-card ui-skel-card">
        <div className="wb-card-head">
          <Skeleton width={88} height={16} />
          <Skeleton width={64} height={11} />
        </div>
        <div className="wb-stats">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="wb-stat">
              <Skeleton width={72} height={10} />
              <Skeleton width={96} height={20} style={{ marginTop: 6 }} />
            </div>
          ))}
        </div>
      </div>

      {/* Payoff shape (5 bars) */}
      <div className="wb-card ui-skel-card">
        <div className="wb-card-head">
          <Skeleton width={108} height={16} />
          <Skeleton width={72} height={11} />
        </div>
        <div className="wb-bars">
          {[60, 35, 14, 35, 60].map((h, i) => (
            <div key={i} className="wb-bar-col">
              <Skeleton
                width="60%"
                height={`${h}%`}
                radius={6}
                style={{ alignSelf: "end" }}
              />
              <Skeleton
                width={48}
                height={9}
                style={{ marginTop: 6 }}
              />
            </div>
          ))}
        </div>
      </div>

      {/* Legs table */}
      <div className="wb-card ui-skel-card">
        <div className="wb-card-head">
          <Skeleton width={50} height={16} />
          <Skeleton width={84} height={11} />
        </div>
        <div className="ui-skel-rows">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="ui-skel-row">
              <Skeleton width={50} height={20} radius={6} />
              <Skeleton width={120} height={12} />
              <Skeleton width={70} height={12} />
              <Skeleton width={70} height={12} />
            </div>
          ))}
        </div>
      </div>

      {/* Payoff scenarios */}
      <div className="wb-card ui-skel-card">
        <div className="wb-card-head">
          <Skeleton width={140} height={16} />
          <Skeleton width={120} height={11} />
        </div>
        <div className="ui-skel-rows">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="ui-skel-row scenario">
              <Skeleton width="55%" height={12} />
              <Skeleton width={70} height={12} />
              <Skeleton width={70} height={12} />
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
