import {
  BaseEdge,
  Position,
  getBezierPath,
  useInternalNode,
  type EdgeProps,
  type InternalNode,
} from '@xyflow/react';

import { edgeAnchors, type Rect, type Side } from '../../lib/graph/edgeAnchors';

// Keep in sync with --tt-node-width / --tt-node-height in design/tokens.css.
const DEFAULT_NODE_WIDTH = 170;
const DEFAULT_NODE_HEIGHT = 120;

const POSITION: Record<Side, Position> = {
  top: Position.Top,
  bottom: Position.Bottom,
  left: Position.Left,
  right: Position.Right,
};

function rectOf(node: InternalNode): Rect {
  return {
    ...node.internals.positionAbsolute,
    width: node.measured.width ?? DEFAULT_NODE_WIDTH,
    height: node.measured.height ?? DEFAULT_NODE_HEIGHT,
  };
}

/**
 * Edge that leaves and enters through whichever sides of the two nodes face
 * each other, so it stays readable when a layout grows sideways or upwards.
 * A child below its parent still gets the usual bottom-to-top curve.
 */
export function FloatingEdge({ id, source, target, markerEnd, style, interactionWidth }: EdgeProps) {
  const sourceNode = useInternalNode(source);
  const targetNode = useInternalNode(target);
  if (!sourceNode || !targetNode) return null;

  const a = edgeAnchors(rectOf(sourceNode), rectOf(targetNode));
  const [path] = getBezierPath({
    sourceX: a.sourceX,
    sourceY: a.sourceY,
    sourcePosition: POSITION[a.sourceSide],
    targetX: a.targetX,
    targetY: a.targetY,
    targetPosition: POSITION[a.targetSide],
  });

  return <BaseEdge id={id} path={path} markerEnd={markerEnd} style={style} interactionWidth={interactionWidth} />;
}
