'use strict';
(() => {
  const $ = id => document.getElementById(id);
  let token = '';
  let page = null;
  let offset = 0;
  let selectedId = '';
  let repository = '';
  let request = null;
  let debounce = null;
  const limit = 25;
  const node = (tag, className, text) => {
    const el = document.createElement(tag);
    if (className) el.className = className;
    if (text !== undefined) el.textContent = text;
    return el;
  };
  const date = value => value ? new Intl.DateTimeFormat(undefined, {dateStyle:'medium'}).format(new Date(value)) : 'No lessons yet';
  const badge = status => node('span', `badge ${status}`, status);
  function notice(message = '') {
    $('notice').textContent = message;
    $('notice').hidden = !message;
  }
  function lock() {
    token = '';
    request?.abort();
    clearTimeout(debounce);
    page = null; selectedId = ''; offset = 0; repository = '';
    $('read-token').value = '';
    $('search').value = ''; $('status').value = '';
    $('library').hidden = true; $('connect-panel').hidden = false;
    $('lock').hidden = true; $('refresh').hidden = true;
    $('lesson-list').replaceChildren(); $('detail').replaceChildren();
    $('repository').replaceChildren(); $('nav-count').textContent = '—';
    notice(); $('read-token').focus();
  }
  function sourceLink(url, label) {
    try {
      const parsed = new URL(url);
      if (parsed.protocol !== 'https:' || parsed.username || parsed.password) throw new Error('Unsafe source URL');
      const a = node('a', '', `${label} ↗`);
      a.href = parsed.href; a.target = '_blank'; a.rel = 'noopener noreferrer';
      return a;
    } catch {
      return node('span', 'source-unavailable', 'Source link unavailable (HTTPS required).');
    }
  }
  function renderDetail(stored) {
    const detail = $('detail'); detail.replaceChildren();
    if (!stored) {
      const empty = node('div', 'empty-state');
      empty.append(node('strong', '', 'Room for the next lesson.'), node('span', '', 'Choose another filter or add evidence-backed candidates through Dreamer.'));
      detail.append(empty); return;
    }
    const lesson = stored.lesson;
    const eyebrow = node('div', 'detail-eyebrow');
    eyebrow.append(node('span', '', 'LESSON DETAIL'), badge(lesson.status));
    detail.append(eyebrow, node('h2', '', lesson.summary), node('code', 'detail-id', lesson.id));
    const warnings = {candidate:'Already available to agents as learned knowledge. Its evidence has not been independently verified; agents should assess applicability to the current task.', verified:'Marked verified in the store. Inspect the evidence and applicability for your current change.', deprecated:'This lesson is deprecated. It remains visible for context and is excluded from MCP searches.'};
    detail.append(node('p', 'candidate-note', warnings[lesson.status]));
    const applies = node('section', 'detail-section');
    applies.append(node('h3', '', 'When to use this'), node('p', '', lesson.applies_when));
    detail.append(applies);
    const provenance = node('section', 'detail-section');
    provenance.append(node('h3', '', 'Provenance'));
    const fields = node('dl', 'provenance');
    for (const [label, value] of [['Proposed by', lesson.provenance.producer], ['Method', lesson.provenance.method], ['Stored', date(stored.stored_at)]]) fields.append(node('dt', '', label), node('dd', '', value));
    provenance.append(fields); detail.append(provenance);
    const evidence = node('section', 'detail-section');
    const title = node('div', 'evidence-title');
    title.append(node('h3', '', 'Follow the evidence'), node('span', '', `${lesson.evidence.length} ${lesson.evidence.length === 1 ? 'source' : 'sources'}`));
    evidence.append(title);
    for (const item of lesson.evidence) {
      const card = node('article', 'evidence-card');
      const type = item.source_url.includes('#discussion_') ? 'Review discussion' : item.source_url.includes('/commit/') ? 'Follow-up commit' : item.source_url.includes('/blob/') ? 'Pinned source' : 'Source';
      const meta = node('div', 'evidence-meta');
      meta.append(node('span', '', type), node('span', '', `PR #${item.pull_request.number}`));
      let sourceLabel = item.pull_request.title;
      try {
        const url = new URL(item.source_url);
        if (url.pathname.includes('/blob/')) sourceLabel = decodeURIComponent(url.pathname.split('/blob/')[1].split('/').slice(1).join('/'));
        else if (url.pathname.includes('/commit/')) sourceLabel = `Commit ${url.pathname.split('/').pop().slice(0, 12)}`;
        else if (type === 'Review discussion') sourceLabel = `Review discussion ${url.hash.replace('#discussion_r', '#')}`;
      } catch { /* Invalid source URLs are rendered as plain text below. */ }
      card.append(meta, sourceLink(item.source_url, sourceLabel), node('p', '', item.description));
      const commit = node('code', '', `Head ${item.pull_request.head_sha}`);
      card.append(commit);
      if (item.commit_sha && item.commit_sha !== item.pull_request.head_sha) card.append(node('code', '', `Evidence ${item.commit_sha}`));
      evidence.append(card);
    }
    detail.append(evidence);
  }
  function render() {
    const {lessons, counts, matching} = page;
    repository = page.repository;
    $('repository').replaceChildren();
    for (const name of page.repositories || [repository]) {
      const option = node('option', '', name); option.value = name;
      $('repository').append(option);
    }
    $('repository').value = repository;
    $('nav-count').textContent = counts.total;
    $('total').textContent = counts.total;
    $('candidates').textContent = counts.candidate;
    $('verified').textContent = counts.verified;
    $('source-prs').textContent = counts.source_prs;
    $('matching-count').textContent = matching;
    $('last-stored').textContent = counts.last_stored_at ? `Last stored ${date(counts.last_stored_at)}` : 'No lessons stored yet';
    $('lesson-list').replaceChildren();
    if (!lessons.some(item => item.lesson.id === selectedId)) selectedId = lessons[0]?.lesson.id || '';
    for (const stored of lessons) {
      const lesson = stored.lesson;
      const card = node('button', 'lesson-card'); card.type = 'button';
      card.setAttribute('aria-pressed', String(lesson.id === selectedId));
      card.setAttribute('aria-controls', 'detail');
      const header = node('span', 'lesson-card-header');
      header.append(badge(lesson.status), node('span', 'card-arrow', '↗'));
      const prs = [...new Set(lesson.evidence.map(item => item.pull_request.number))];
      const footer = node('span', 'card-footer');
      footer.append(node('span', '', prs.map(number => `PR #${number}`).join(', ')), node('span', '', `${lesson.evidence.length} ${lesson.evidence.length === 1 ? 'source' : 'sources'}`), node('span', '', date(stored.stored_at)));
      card.append(header, node('span', 'card-title', lesson.summary), node('span', 'card-id', lesson.id), footer);
      card.addEventListener('click', () => {
        selectedId = lesson.id;
        for (const other of $('lesson-list').children) other.setAttribute('aria-pressed', String(other === card));
        renderDetail(stored);
      });
      $('lesson-list').append(card);
    }
    if (!lessons.length) {
      const empty = node('div', 'empty-state');
      empty.append(node('strong', '', matching ? 'No lessons on this page.' : counts.total ? 'No matching lessons.' : 'Your library starts here.'), node('span', '', counts.total ? 'Try a different search or status.' : 'Published candidates will appear here when you refresh.'));
      $('lesson-list').append(empty);
    }
    renderDetail(lessons.find(item => item.lesson.id === selectedId));
    $('previous').disabled = offset === 0;
    $('next').disabled = offset + lessons.length >= matching;
    $('page-info').textContent = matching ? `${lessons.length ? offset + 1 : 0}–${offset + lessons.length} of ${matching} lessons` : '0 lessons';
  }
  async function load() {
    if (!token) return;
    request?.abort();
    const active = new AbortController(); request = active;
    $('refresh').disabled = true; $('connect').disabled = true;
    $('connect').textContent = 'Opening…';
    $('results').setAttribute('aria-busy', 'true');
    notice();
    const params = new URLSearchParams({q:$('search').value, limit:String(limit), offset:String(offset)});
    if ($('status').value) params.set('status', $('status').value);
    if (repository) params.set('repository', repository);
    try {
      const response = await fetch(`/api/knowledge?${params}`, {headers:{Authorization:`Bearer ${token}`}, cache:'no-store', credentials:'omit', signal:active.signal, redirect:'error'});
      if (request !== active || active.signal.aborted) return;
      if (response.status === 401) { lock(); notice('The read token was not accepted. Check the token and try again.'); return; }
      if (!response.ok) throw new Error('Library unavailable');
      const opening = !page;
      const incoming = await response.json();
      if (request !== active || active.signal.aborted) return;
      if (repository && incoming.repository !== repository) throw new Error('Repository response mismatch');
      if (incoming.lessons.some(item => item.lesson.repository !== incoming.repository)) throw new Error('Repository response mismatch');
      page = incoming;
      render();
      $('connect-panel').hidden = true; $('library').hidden = false;
      $('lock').hidden = false; $('refresh').hidden = false;
      if (opening) $('search').focus();
    } catch (error) {
      if (request === active && error.name !== 'AbortError') notice(page ? 'Could not refresh. You are viewing the last loaded data; try again.' : 'Could not load the library. Check the service connection and try again.');
    } finally {
      if (request === active) {
        $('refresh').disabled = false; $('connect').disabled = false;
        $('connect').textContent = 'Open library ↗';
        $('results').setAttribute('aria-busy', 'false');
      }
    }
  }
  $('connect-form').addEventListener('submit', event => {
    event.preventDefault(); token = $('read-token').value.trim(); $('read-token').value = ''; offset = 0; load();
  });
  $('repository').addEventListener('change', () => {
    clearTimeout(debounce); request?.abort();
    repository = $('repository').value; page = null; selectedId = ''; offset = 0;
    $('search').value = ''; $('status').value = '';
    $('lesson-list').replaceChildren(node('div', 'empty-state', 'Loading repository knowledge…'));
    $('detail').replaceChildren();
    for (const id of ['total','candidates','verified','source-prs','matching-count','nav-count']) $(id).textContent = '—';
    $('last-stored').textContent = ''; $('page-info').textContent = '';
    $('previous').disabled = true; $('next').disabled = true;
    load();
  });
  $('lock').addEventListener('click', lock);
  $('refresh').addEventListener('click', () => load());
  $('search').addEventListener('input', () => { clearTimeout(debounce); request?.abort(); offset = 0; debounce = setTimeout(load, 250); });
  $('status').addEventListener('change', () => { clearTimeout(debounce); offset = 0; load(); });
  $('previous').addEventListener('click', () => { offset = Math.max(0, offset - limit); load(); });
  $('next').addEventListener('click', () => { offset += limit; load(); });
})();
