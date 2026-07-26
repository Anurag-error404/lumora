import { useState } from "react";
import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { fileSrc, type Person } from "../../lib/tauri";

function PersonCover({ person }: { person: Person }) {
  return (
    <div className="person-cover-media">
      <SafeImage
        src={person.coverCropPath ? fileSrc(person.coverCropPath) : null}
        alt=""
        loading="lazy"
        fallback={<MediaFallback type="album" />}
      />
    </div>
  );
}

function faceLabel(count: number) {
  return `${count} ${count === 1 ? "face" : "faces"}`;
}

/** Discover → People: clustered faces with name / merge / ignore actions. */
export function PeopleView({
  people,
  ignoredPeople,
  onOpenPerson,
  onNamePerson,
  onSetIgnored,
  onRefresh,
}: {
  people: Person[];
  ignoredPeople: Person[];
  onOpenPerson: (personId: string) => void;
  onNamePerson: (person: Person) => void;
  onSetIgnored: (personId: string, ignored: boolean) => void;
  onRefresh: () => void;
}) {
  const [showIgnored, setShowIgnored] = useState(false);

  return (
    <div className="people-page">
      <PageHeader
        title="People"
        description="Faces detected on-device are grouped automatically. Name someone once to find every photo of them."
        actions={
          <>
            {ignoredPeople.length > 0 ? (
              <button
                type="button"
                onClick={() => setShowIgnored((v) => !v)}
                aria-pressed={showIgnored}
              >
                {showIgnored ? "Hide ignored" : `Ignored (${ignoredPeople.length})`}
              </button>
            ) : null}
            <button type="button" onClick={onRefresh}>
              Refresh
            </button>
          </>
        }
      />
      {people.length === 0 ? (
        <EmptyState
          icon="person"
          title="No people yet"
          description="Install face models in Settings → AI Features, then LUMORA groups faces in the background."
        />
      ) : (
        <div className="person-cover-grid">
          {people.map((person) => (
            <article key={person.id} className="person-cover-card">
              <button
                type="button"
                className="person-cover-open"
                onClick={() => onOpenPerson(person.id)}
              >
                <PersonCover person={person} />
                <div className="person-cover-info">
                  <span className="person-cover-name">
                    {person.name?.trim() || "Unnamed"}
                  </span>
                  <span className="muted">{faceLabel(person.faceCount)}</span>
                </div>
              </button>
              <div className="person-cover-actions">
                <button
                  type="button"
                  className="person-name-button"
                  onClick={() => onNamePerson(person)}
                >
                  {person.name?.trim() ? "Rename / merge" : "Name person"}
                </button>
                <button
                  type="button"
                  className="person-ignore-button"
                  title="Hide this person and stop surfacing this face in future imports"
                  onClick={() => onSetIgnored(person.id, true)}
                >
                  Ignore
                </button>
              </div>
            </article>
          ))}
        </div>
      )}

      {showIgnored && ignoredPeople.length > 0 ? (
        <section className="people-ignored">
          <h3>Ignored people</h3>
          <p className="muted">
            These faces stay hidden from People and search, including in photos
            you import later.
          </p>
          <div className="person-cover-grid">
            {ignoredPeople.map((person) => (
              <article
                key={person.id}
                className="person-cover-card person-cover-card-ignored"
              >
                <PersonCover person={person} />
                <div className="person-cover-info">
                  <span className="person-cover-name">
                    {person.name?.trim() || "Unnamed"}
                  </span>
                  <span className="muted">{faceLabel(person.faceCount)}</span>
                </div>
                <button
                  type="button"
                  className="person-name-button"
                  onClick={() => onSetIgnored(person.id, false)}
                >
                  Stop ignoring
                </button>
              </article>
            ))}
          </div>
        </section>
      ) : null}
    </div>
  );
}
