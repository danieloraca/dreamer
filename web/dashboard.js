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
  let detailRequest = null;
  let activityRequest = null;
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
    request?.abort(); detailRequest?.abort(); activityRequest?.abort();
    clearTimeout(debounce);
    page = null; selectedId = ''; offset = 0; repository = '';
    $('read-token').value = '';
    $('search').value = ''; $('status').value = '';
    $('library').hidden = true; $('connect-panel').hidden = false;
    $('lock').hidden = true; $('refresh').hidden = true;
    $('lesson-list').replaceChildren(); $('detail').replaceChildren();
    $('repository').replaceChildren(); $('nav-count').textContent = '—';
    $('activity-list').replaceChildren(); $('activity-summary').textContent = ''; $('backup-status').textContent = ''; $('activity-more').hidden = true;
    notice(); $('read-token').focus();
  }
  function sourceLink(url, label) {
    try {
      const parsed = new URL(url);
      if ((parsed.protocol !== 'https:' && !/^codex:\/\/threads\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(url)) || parsed.username || parsed.password) throw new Error('Unsafe source URL');
      const a = node('a', '', `${label} ↗`);
      a.href = parsed.href; a.target = '_blank'; a.rel = 'noopener noreferrer';
      return a;
    } catch {
      return node('span', 'source-unavailable', 'Source link unavailable (HTTPS or a valid Codex thread required).');
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
    const scope = lesson.scope || {};
    for (const [label, values] of [['Components',scope.components],['Paths',scope.paths],['Excluded terms',scope.excluded_terms]]) {
      if (values?.length) detail.append(node('p','scope-values',`${label}: ${values.join(', ')}`));
    }
    const lifecycle = stored.lifecycle || {};
    function relation(label,id) {
      const button = node('button','text-button relation-button',`${label}: ${id} ↗`);
      button.type = 'button'; button.addEventListener('click',()=>openLesson(id)); detail.append(button);
    }
    if (lifecycle.superseded_by) relation('Replaced by',lifecycle.superseded_by);
    if (lifecycle.retirement_reason) detail.append(node('p','candidate-note',`Reason: ${lifecycle.retirement_reason}`));
    for (const id of lifecycle.conflicts || []) relation('Conflicts with',id);
    if (lifecycle.conflicts_truncated) detail.append(node('p','metric-note','More conflict links are available in history.'));
    const provenance = node('section', 'detail-section');
    provenance.append(node('h3', '', 'Provenance'));
    const fields = node('dl', 'provenance');
    for (const [label, value] of [['Proposed by', lesson.provenance.producer], ['Method', lesson.provenance.method], ['Stored', date(stored.stored_at)]]) fields.append(node('dt', '', label), node('dd', '', value));
    provenance.append(fields); detail.append(provenance);
    const sources = [...lesson.evidence, ...(lesson.conversation_evidence || [])];
    const evidence = node('section', 'detail-section');
    const title = node('div', 'evidence-title');
    title.append(node('h3', '', 'Follow the evidence'), node('span', '', `${sources.length} ${sources.length === 1 ? 'source' : 'sources'}`));
    evidence.append(title);
    for (const item of sources) {
      if (item.provider === 'codex') {
        const card=node('article','evidence-card');
        card.append(node('div','evidence-meta',`Codex conversation · ${item.claim} · ${item.kind}`),sourceLink(item.source_url,'Open source chat'),node('p','',item.description),node('code','',`Turn ${item.turn_id} · Item ${item.item_id}`),node('code','',`Digest ${item.content_sha256}`),node('span','',`Observed ${date(item.observed_at)} · Excerpt, not independently verified`));
        evidence.append(card); continue;
      }
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
    const history = node('section','detail-section'); history.id = 'lesson-history';
    history.append(node('h3','','Correction history'));
    const load = node('button','text-button','Load history'); load.type='button'; load.addEventListener('click',()=>openLesson(lesson.id));
    history.append(load); detail.append(history);
  }
  async function authenticated(path, active) {
    const response = await fetch(path,{headers:{Authorization:`Bearer ${token}`},cache:'no-store',credentials:'omit',redirect:'error',signal:active.signal});
    if (response.status===401) {lock();notice('The read token was not accepted.');throw new Error('Unauthorized');}
    if (!response.ok) throw new Error('Read unavailable');
    return response.json();
  }
  async function openLesson(id, nextOffset=0) {
    if (!token) return;
    detailRequest?.abort(); const active=new AbortController(); detailRequest=active;
    const scope=repository;
    try {
      const value=await authenticated(`/api/lesson?${new URLSearchParams({repository:scope,id,offset:String(nextOffset),limit:'10'})}`,active);
      if (active.signal.aborted || detailRequest!==active || repository!==scope) return;
      if (value.repository!==scope || (value.stored && value.stored.lesson.repository!==scope)) throw new Error('Scope mismatch');
      if (!value.stored) {notice('This related lesson is not available in this repository.');return;}
      selectedId=id;
      renderDetail(value.stored);
      for (const card of $('lesson-list').children) card.setAttribute('aria-pressed',String(card.dataset.id===id));
      const section=$('lesson-history'); section.replaceChildren(node('h3','','Correction history'));
      if (!value.history.changes.length) section.append(node('p','metric-note','No correction events on this page.'));
      for (const event of value.history.changes) {
        const change=event.change; const row=node('div','history-row');
        row.append(node('strong','',change.action),node('p','',change.reason),node('span','',`${date(event.stored_at)} · ${change.producer}`));
        for (const related of [change.source_id,change.target_id].filter(x=>x && x!==id)) {
          const button=node('button','text-button relation-button',`View ${related} ↗`);button.type='button';button.addEventListener('click',()=>openLesson(related));row.append(button);
        }
        for (const evidence of [...(change.evidence || []),...(change.conversation_evidence || [])]) row.append(sourceLink(evidence.source_url,'Correction evidence'));
        section.append(row);
      }
      for (const [label,offset] of [['Previous events',nextOffset>0?Math.max(0,nextOffset-10):null],['More events',value.history.next_offset]]) {
        if (offset!==null) {const button=node('button','text-button',label);button.type='button';button.addEventListener('click',()=>openLesson(id,offset));section.append(button);}
      }
    } catch(error) {if(detailRequest===active && error.name!=='AbortError' && token) notice('Could not load lesson history. Refresh and try again.');}
  }
  async function loadActivity(nextOffset=0) {
    if (!token || !repository) return;
    activityRequest?.abort();const active=new AbortController();activityRequest=active;const scope=repository;
    try {
      const value=await authenticated(`/api/activity?${new URLSearchParams({repository:scope,limit:'5',offset:String(nextOffset)})}`,active);
      if (active.signal.aborted || activityRequest!==active || repository!==scope) return;
      if (value.repository!==scope) throw new Error('Scope mismatch');
      const summary=value.summary;
      $('activity-summary').textContent=`${summary.last_learning_report_at?'Last successful learning report '+date(summary.last_learning_report_at):'No successful learning report received yet.'} · ${summary.failures} failed · ${summary.incomplete} without a completion report (latest ${summary.retained} retained).`;
      $('activity-list').replaceChildren();
      if (!value.runs.length) $('activity-list').append(node('p','metric-note','No activity reports on this page. Existing knowledge predates reporting.'));
      for (const run of value.runs) {
        const counts=run.inserted===null || run.inserted===undefined?'Outcome counts unknown':`${run.inserted} new · ${run.reused} reused · ${run.changed} corrected`;
        $('activity-list').append(node('div',`run-row ${run.state}`,`${run.phase} · ${run.state==='started'?'completion not reported':run.state} · ${run.pr?'PR #'+run.pr:run.conversation?'Codex chat':'source revision'} · received ${date(run.reported_at)}${run.phase==='capture'?'':' · '+counts}`));
      }
      $('activity-more').hidden=value.next_offset===null;
      $('activity-more').onclick=()=>loadActivity(value.next_offset);
      const backup=value.backup;const last=backup.last_success;
      $('backup-status').textContent=backup.state==='not_yet_reported'?'No backup report yet. Run make backup on the Pi.':backup.state==='unreadable'?'Backup report unavailable. Inspect the backup service on the Pi.':`${backup.state==='failed'?'Last attempt failed. ':''}${last?`Last checked copy ${date(last.completed_at)}${last.stale?' (over 48 hours old)':''} · schema ${last.schema} · ${last.retained_count??'unknown'} ${last.retained_count===1?'copy':'copies'} retained.`:'No checked successful copy reported.'}`;
    } catch(error) {if(activityRequest===active && error.name!=='AbortError' && token){$('activity-summary').textContent='Activity unavailable. The Pi may need an update; knowledge browsing still works.';$('backup-status').textContent='Backup status unavailable.';$('activity-list').replaceChildren();$('activity-more').hidden=true;}}
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
    $('source-prs').textContent = counts.source_prs + (counts.source_chats || 0);
    $('source-note').textContent = `${counts.source_prs} PRs · ${counts.source_chats || 0} chats`;
    $('matching-count').textContent = matching;
    $('last-stored').textContent = counts.last_stored_at ? `Last stored ${date(counts.last_stored_at)}` : 'No lessons stored yet';
    $('lesson-list').replaceChildren();
    if (!lessons.some(item => item.lesson.id === selectedId)) selectedId = lessons[0]?.lesson.id || '';
    for (const stored of lessons) {
      const lesson = stored.lesson;
      const card = node('button', 'lesson-card'); card.type = 'button'; card.dataset.id=lesson.id;
      card.setAttribute('aria-pressed', String(lesson.id === selectedId));
      card.setAttribute('aria-controls', 'detail');
      const header = node('span', 'lesson-card-header');
      header.append(badge(lesson.status), node('span', 'card-arrow', '↗'));
      const prs = [...new Set(lesson.evidence.map(item => item.pull_request.number))];
      const sources = [...lesson.evidence,...(lesson.conversation_evidence || [])];
      const chats = new Set((lesson.conversation_evidence || []).map(item=>item.thread_id)).size;
      const footer = node('span', 'card-footer');
      footer.append(node('span', '', [...prs.map(number => `PR #${number}`),...(chats?[`${chats} Codex ${chats===1?'chat':'chats'}`]:[])].join(', ')), node('span', '', `${sources.length} ${sources.length === 1 ? 'source' : 'sources'}`), node('span', '', date(stored.stored_at)));
      card.append(header, node('span', 'card-title', lesson.summary), node('span', 'card-id', lesson.id), footer);
      card.addEventListener('click', () => {
        selectedId = lesson.id;
        for (const other of $('lesson-list').children) other.setAttribute('aria-pressed', String(other === card));
        openLesson(lesson.id);
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
      detailRequest?.abort();
      render();
      loadActivity();
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
    clearTimeout(debounce); request?.abort(); detailRequest?.abort(); activityRequest?.abort();
    repository = $('repository').value; page = null; selectedId = ''; offset = 0;
    $('search').value = ''; $('status').value = '';
    $('lesson-list').replaceChildren(node('div', 'empty-state', 'Loading repository knowledge…'));
    $('detail').replaceChildren();
    $('activity-list').replaceChildren(); $('activity-summary').textContent='Loading activity…'; $('backup-status').textContent='Loading backup status…'; $('activity-more').hidden=true;
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
