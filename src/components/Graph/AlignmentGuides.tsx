import { useViewport } from "@xyflow/react";

export interface AlignmentGuide {
  type: "horizontal" | "vertical";
  position: number; // x for vertical lines, y for horizontal lines
}

interface AlignmentGuidesProps {
  guides: AlignmentGuide[];
}

export function AlignmentGuides({ guides }: AlignmentGuidesProps) {
  const { x, y, zoom } = useViewport();

  if (guides.length === 0) return null;

  // Calculate viewport bounds for guide line extent
  // We use a large fixed value to ensure guides span the visible area
  const extent = 10000;

  return (
    <svg
      className="alignment-guides-container"
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        width: "100%",
        height: "100%",
        pointerEvents: "none",
        overflow: "visible",
      }}
    >
      <g
        transform={`translate(${x}, ${y}) scale(${zoom})`}
        style={{ transformOrigin: "0 0" }}
      >
        {guides.map((guide, index) => {
          if (guide.type === "horizontal") {
            return (
              <line
                key={`h-${index}`}
                className="alignment-guide"
                x1={-extent}
                y1={guide.position}
                x2={extent}
                y2={guide.position}
              />
            );
          } else {
            return (
              <line
                key={`v-${index}`}
                className="alignment-guide"
                x1={guide.position}
                y1={-extent}
                x2={guide.position}
                y2={extent}
              />
            );
          }
        })}
      </g>
    </svg>
  );
}
