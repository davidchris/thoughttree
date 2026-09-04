import { TOOL_TITLE_MAX_LENGTH, isWebUrl } from '@thoughttree/graph-model';
import type {
  ToolActivity,
  TurnActivity,
  TurnProvenance,
  TurnReference,
} from '@thoughttree/graph-model';

interface ProvenanceProps {
  provenance: TurnProvenance;
  content?: string;
}

function countLabel(count: number, singular: string, plural = `${singular}s`) {
  return `${count} ${count === 1 ? singular : plural}`;
}

function Reference({ reference, cited }: { reference: TurnReference; cited: boolean }) {
  const relations = reference.relations.join(' · ');

  if (reference.type === 'url') {
    const sourceKind = reference.is_search_result ? 'Search result' : 'Fetched page';

    return (
      <li className="side-panel-provenance-reference">
        {reference.index !== undefined && (
          <span className="side-panel-provenance-reference-meta">
            Reference {reference.index} · {cited ? 'Cited' : 'Consulted'} · {sourceKind}
          </span>
        )}
        {isWebUrl(reference.url) ? (
          <a href={reference.url} target="_blank" rel="noopener noreferrer">
            {reference.title || reference.url}
          </a>
        ) : (
          <span>{reference.title || reference.url}</span>
        )}
        {reference.title && <span className="side-panel-provenance-location">{reference.url}</span>}
        <span className="side-panel-provenance-relations">{relations}</span>
      </li>
    );
  }

  return (
    <li className="side-panel-provenance-reference">
      <span>{reference.scope === 'vault' ? reference.path : reference.displayName}</span>
      <span className="side-panel-provenance-relations">{relations}</span>
    </li>
  );
}

function titleCase(value: string) {
  return `${value.charAt(0).toUpperCase()}${value.slice(1)}`;
}

function ToolDetails({ activity }: { activity: ToolActivity }) {
  const title = activity.title.slice(0, TOOL_TITLE_MAX_LENGTH);
  const wasTruncated = activity.titleTruncated || activity.title.length > TOOL_TITLE_MAX_LENGTH;

  return (
    <details className="side-panel-provenance-activity">
      <summary>
        {titleCase(activity.kind)} · {titleCase(activity.status)}
      </summary>
      <div className="side-panel-provenance-activity-detail">
        <span>{title}</span>
        {wasTruncated && <span className="side-panel-provenance-truncated">Title truncated</span>}
        {activity.titleRedacted && (
          <span className="side-panel-provenance-truncated">Title replaced by a summary</span>
        )}
      </div>
    </details>
  );
}

function Activity({ activity }: { activity: TurnActivity }) {
  switch (activity.type) {
    case 'commentary':
      return (
        <details className="side-panel-provenance-activity">
          <summary>Assistant commentary</summary>
          <p className="side-panel-provenance-activity-detail">{activity.content}</p>
        </details>
      );
    case 'tool':
      return <ToolDetails activity={activity} />;
    case 'unknown':
      return (
        <div className="side-panel-provenance-unknown">
          {activity.providerType} · {activity.label}
        </div>
      );
  }
}

export function Provenance({ provenance, content = '' }: ProvenanceProps) {
  const citedIndexes = new Set(
    Array.from(content.matchAll(/【(\d+)】/gu), (match) => Number(match[1]))
  );
  const references = provenance.references
    .map((reference, originalIndex) => ({ reference, originalIndex }))
    .sort((left, right) => {
      const leftIndex = left.reference.type === 'url' ? left.reference.index : undefined;
      const rightIndex = right.reference.type === 'url' ? right.reference.index : undefined;
      if (leftIndex === undefined && rightIndex === undefined) {
        return left.originalIndex - right.originalIndex;
      }
      if (leftIndex === undefined) return 1;
      if (rightIndex === undefined) return -1;
      return leftIndex - rightIndex;
    })
    .map(({ reference }) => reference);
  const referenceIndexes = new Set(
    references.flatMap((reference) =>
      reference.type === 'url' && reference.index !== undefined ? [reference.index] : []
    )
  );
  const missingCitationIndexes = [...citedIndexes]
    .filter((index) => !referenceIndexes.has(index))
    .sort((left, right) => left - right);
  const sourceCount = provenance.references.filter(
    (reference) => reference.type === 'url'
  ).length;
  const fileCount = provenance.references.length - sourceCount;

  return (
    <details className="side-panel-provenance">
      <summary>
        Provenance · {countLabel(sourceCount, 'source')} · {countLabel(fileCount, 'file')} ·{' '}
        {countLabel(provenance.activity.length, 'activity', 'activities')}
      </summary>
      <div className="side-panel-provenance-content">
        {provenance.completeness !== 'complete' && (
          <p className="side-panel-provenance-warning">Some Turn evidence may be missing.</p>
        )}
        <h3>References</h3>
        <ol className="side-panel-provenance-list">
          {references.map((reference, index) => (
            <Reference
              key={index}
              reference={reference}
              cited={reference.type === 'url' && reference.index !== undefined
                ? citedIndexes.has(reference.index)
                : reference.relations.includes('cited')}
            />
          ))}
        </ol>
        {missingCitationIndexes.length > 0 && (
          <p className="side-panel-provenance-warning">
            {missingCitationIndexes.map((index) => `Citation marker 【${index}】 has no matching reference.`).join(' ')}
          </p>
        )}
        {references.length === 0 && (
          <p className="side-panel-provenance-empty">No references recorded.</p>
        )}
        <h3>Turn activity</h3>
        <ol className="side-panel-provenance-list">
          {provenance.activity.map((activity, index) => (
            <li key={index}>
              <Activity activity={activity} />
            </li>
          ))}
        </ol>
        {provenance.activity.length === 0 && (
          <p className="side-panel-provenance-empty">No Turn activity recorded.</p>
        )}
      </div>
    </details>
  );
}
