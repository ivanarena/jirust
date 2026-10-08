package ui

import (
	"fmt"
	"os/exec"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/ivanarena/jiraui/internal/cache"
	"github.com/ivanarena/jiraui/internal/config"
	"github.com/ivanarena/jiraui/internal/jira"
)

type listMsg struct {
	seq    int
	items  []jira.Issue
	cached bool
	err    error
}
type detailMsg struct {
	key    string
	issue  jira.Issue
	cached bool
	err    error
}
type selectTick struct {
	key string
	seq int
}
type actionMsg struct {
	action jira.Action
	err    error
}
type noticeMsg struct {
	text string
	err  error
}
type boardsMsg struct {
	boards []jira.Board
	err    error
}
type sprintsMsg struct {
	board   jira.Board
	sprints []jira.Sprint
	err     error
}
type commentsMsg struct {
	key      string
	comments []jira.Comment
	err      error
}
type record struct {
	issue   jira.Issue
	updated time.Time
}
type Dialog struct {
	kind      string
	title     string
	labels    []string
	inputs    []textinput.Model
	area      textarea.Model
	index     int
	options   []string
	values    []string
	targets   []jira.Target
	boards    []jira.Board
	sprints   []jira.Sprint
	action    jira.Action
	errorView viewport.Model
}
type Model struct {
	client             jira.Backend
	cache              *cache.Store
	cfg                config.Config
	readOnly           bool
	limit              int
	width, height      int
	view               string
	raw, search, local string
	searchInput        textinput.Model
	searching          bool
	items              []jira.Issue
	visible            []int
	selected           int
	listOffset         int
	focus              string
	sidebar            int
	boardMode          bool
	boardID, sprintID  int
	sprintName         string
	details            map[string]record
	pending            map[string]bool
	detailKey          string
	history            []string
	viewport           viewport.Model
	detailLines        []string
	lineLinks          map[int]jira.Target
	detailFor          string
	detailWidth        int
	dialog             *Dialog
	help               bool
	loading, mutating  bool
	seq, selectionSeq  int
	spinner            spinner.Model
	status             string
	lastError          string
	isError            bool
}

func New(client jira.Backend, store *cache.Store, cfg config.Config, readOnly bool, limit int, raw string) *Model {
	s := spinner.New()
	s.Spinner = spinner.Dot
	input := textinput.New()
	input.Placeholder = "Search loaded tickets; Enter searches Jira"
	input.CharLimit = 500
	return &Model{client: client, cache: store, cfg: cfg, readOnly: readOnly, limit: limit, view: "mine", raw: raw, focus: "list", loading: true, details: map[string]record{}, pending: map[string]bool{}, viewport: viewport.New(40, 20), spinner: s, searchInput: input, lineLinks: map[int]jira.Target{}, status: "Loading tickets…"}
}
func (m Model) Init() tea.Cmd { return tea.Batch(m.fetch(0), m.spinner.Tick) }
func (m Model) query() string { return jira.Query(m.view, m.cfg.Filters, m.search, m.raw) }
func (m Model) cacheKey() string {
	return fmt.Sprintf("list:%d:%d:%d:%s", m.boardID, m.sprintID, m.limit, m.query())
}
func (m Model) fetch(seq int) tea.Cmd {
	client, store, key, query, limit, board, sprint := m.client, m.cache, m.cacheKey(), m.query(), m.limit, m.boardID, m.sprintID
	var cached []jira.Issue
	_, ok := store.Get(key, &cached)
	network := func() tea.Msg {
		var items []jira.Issue
		var err error
		if sprint > 0 {
			items, err = client.SprintIssues(board, sprint, query, limit)
		} else {
			items, err = client.Search(query, limit)
		}
		if err == nil {
			if e := store.Put(key, items); e != nil {
				return listMsg{seq: seq, items: items, err: fmt.Errorf("tickets loaded, but local cache failed: %w", e)}
			}
		}
		return listMsg{seq: seq, items: items, err: err}
	}
	if ok {
		return tea.Sequence(func() tea.Msg { return listMsg{seq: seq, items: cached, cached: true} }, network)
	}
	return network
}
func (m *Model) reload(clear bool) tea.Cmd {
	m.seq++
	m.loading = true
	if clear {
		m.items = nil
		m.visible = nil
		m.selected = 0
		m.listOffset = 0
		m.detailKey = ""
		m.history = nil
	}
	return tea.Batch(m.fetch(m.seq), m.spinner.Tick)
}
func (m *Model) rebuildVisible() {
	m.visible = nil
	needle := strings.ToLower(strings.TrimSpace(m.local))
	for n, i := range m.items {
		text := i.Key + " " + i.Fields.Summary + " " + i.Fields.Status.Name + " " + i.Fields.Assignee.String() + " " + i.Fields.Reporter.String()
		if needle == "" || strings.Contains(strings.ToLower(text), needle) {
			m.visible = append(m.visible, n)
		}
	}
	m.selected = max(0, min(m.selected, len(m.visible)-1))
}
func (m Model) selectedIssue() (jira.Issue, bool) {
	if m.selected < 0 || m.selected >= len(m.visible) {
		return jira.Issue{}, false
	}
	return m.items[m.visible[m.selected]], true
}
func (m Model) activeIssue() (jira.Issue, bool) {
	if m.focus == "detail" && m.detailKey != "" {
		if r, ok := m.details[m.detailKey]; ok {
			return r.issue, true
		}
		return jira.Issue{Key: m.detailKey}, true
	}
	return m.selectedIssue()
}
func (m *Model) selection() tea.Cmd {
	i, ok := m.selectedIssue()
	if !ok {
		return nil
	}
	m.detailKey = i.Key
	m.history = nil
	m.detailFor = ""
	m.selectionSeq++
	key, seq := i.Key, m.selectionSeq
	return tea.Tick(180*time.Millisecond, func(time.Time) tea.Msg { return selectTick{key, seq} })
}
func (m *Model) detail(key string, force bool) tea.Cmd {
	if m.pending[key] {
		return nil
	}
	if r, ok := m.details[key]; ok && !force && time.Since(r.updated) < 2*time.Minute {
		return nil
	}
	m.pending[key] = true
	client, store := m.client, m.cache
	var issue jira.Issue
	stamp, hit := store.Get("detail:"+key, &issue)
	network := func() tea.Msg {
		i, err := client.Get(key)
		if err == nil {
			_ = store.Put("detail:"+key, i)
		}
		return detailMsg{key: key, issue: i, err: err}
	}
	if hit && !force {
		if time.Since(stamp) < 2*time.Minute {
			return func() tea.Msg { return detailMsg{key: key, issue: issue} }
		}
		return tea.Sequence(func() tea.Msg { return detailMsg{key: key, issue: issue, cached: true} }, network)
	}
	return tea.Batch(network, m.spinner.Tick)
}
func (m *Model) openInternal(key string) tea.Cmd {
	if !jira.IssueKey.MatchString(key) {
		m.notice("Invalid linked ticket key", true)
		return nil
	}
	key = strings.ToUpper(key)
	if m.detailKey != key && m.detailKey != "" {
		m.history = append(m.history, m.detailKey)
	}
	m.detailKey = key
	m.focus = "detail"
	m.detailFor = ""
	m.viewport.GotoTop()
	return m.detail(key, false)
}
func (m *Model) notice(s string, bad bool) {
	m.status = jira.SafeLine(s)
	m.isError = bad
	if bad {
		m.lastError = jira.Safe(s)
	}
}
func openURL(s string) tea.Cmd {
	return func() tea.Msg {
		if err := jira.ValidateURL(s); err != nil {
			return noticeMsg{err: err}
		}
		program, args := "xdg-open", []string{s}
		if runtime.GOOS == "darwin" {
			program = "open"
		}
		if runtime.GOOS == "windows" {
			program = "rundll32"
			args = []string{"url.dll,FileProtocolHandler", s}
		}
		err := exec.Command(program, args...).Run()
		return noticeMsg{text: "Opened link in browser", err: err}
	}
}
func (m *Model) openTarget(t jira.Target) tea.Cmd {
	if t.Key != "" {
		return m.openInternal(t.Key)
	}
	return openURL(t.URL)
}
func (m *Model) switchView(index int) tea.Cmd {
	views := []string{"mine", "all", "backlog", "sprint", "board"}
	m.sidebar = index
	m.local = ""
	m.search = ""
	m.raw = ""
	m.boardID = 0
	m.sprintID = 0
	m.sprintName = ""
	m.focus = "list"
	if index < len(views) {
		m.view = views[index]
		m.boardMode = m.view == "board"
	} else if index-len(views) < len(m.cfg.Saved) {
		m.view = "saved"
		m.raw = m.cfg.Saved[index-len(views)].JQL
		m.boardMode = false
	}
	return m.reload(true)
}
func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch x := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = x.Width
		m.height = x.Height
		m.detailFor = ""
		return m, nil
	case listMsg:
		if x.seq != m.seq {
			return m, nil
		}
		m.loading = x.cached
		if x.err != nil {
			m.loading = false
			m.notice(x.err.Error(), true)
			if x.items == nil {
				return m, nil
			}
		}
		old, _ := m.selectedIssue()
		m.items = x.items
		m.rebuildVisible()
		for pos, n := range m.visible {
			if m.items[n].Key == old.Key {
				m.selected = pos
				break
			}
		}
		if x.err == nil {
			suffix := ""
			if x.cached {
				suffix = " · cached, refreshing"
			}
			if len(x.items) == m.limit {
				suffix += " · query limit reached; narrow filters"
			}
			m.notice(fmt.Sprintf("%d tickets%s", len(x.items), suffix), false)
		}
		if len(m.history) > 0 && m.focus == "detail" {
			return m, nil
		}
		return m, m.selection()
	case selectTick:
		if x.seq == m.selectionSeq && x.key == m.detailKey {
			return m, m.detail(x.key, false)
		}
		return m, nil
	case detailMsg:
		if !x.cached {
			delete(m.pending, x.key)
		}
		if x.err != nil {
			if x.key == m.detailKey {
				m.notice(x.err.Error(), true)
			}
			return m, nil
		}
		m.details[x.key] = record{issue: x.issue, updated: time.Now()}
		if x.cached {
			r := m.details[x.key]
			r.updated = time.Time{}
			m.details[x.key] = r
		}
		if x.key == m.detailKey {
			m.detailFor = ""
		}
		return m, nil
	case actionMsg:
		m.mutating = false
		// A timeout may still have reached Jira: invalidate before presenting failure.
		delete(m.details, x.action.Key)
		m.cache.Remove("detail:" + x.action.Key)
		m.cache.Remove(m.cacheKey())
		if x.err != nil {
			m.notice(x.err.Error(), true)
			return m, m.detail(x.action.Key, true)
		}
		m.notice("Updated "+x.action.Key, false)
		return m, tea.Batch(m.reload(false), m.detail(x.action.Key, true))
	case noticeMsg:
		if x.err != nil {
			m.notice(x.err.Error(), true)
		} else {
			m.notice(x.text, false)
		}
		return m, nil
	case boardsMsg:
		m.loading = false
		if x.err != nil {
			m.notice(x.err.Error(), true)
			return m, nil
		}
		if len(x.boards) == 0 {
			m.notice("No boards found for the current project", false)
			return m, nil
		}
		d := &Dialog{kind: "boards", title: "Select board · then choose a sprint", boards: x.boards}
		for _, b := range x.boards {
			d.options = append(d.options, fmt.Sprintf("%d · %s (%s)", b.ID, jira.SafeLine(b.Name), b.Type))
		}
		m.dialog = d
		return m, nil
	case sprintsMsg:
		m.loading = false
		if x.err != nil {
			m.notice(x.err.Error(), true)
			return m, nil
		}
		if len(x.sprints) == 0 {
			m.notice("No active/future sprints. Kanban boards may not have sprints.", false)
			return m, nil
		}
		m.boardID = x.board.ID
		d := &Dialog{kind: "sprints", title: jira.SafeLine(x.board.Name) + " · select sprint", sprints: x.sprints}
		for _, s := range x.sprints {
			d.options = append(d.options, fmt.Sprintf("%d · %s (%s)", s.ID, jira.SafeLine(s.Name), s.State))
		}
		m.dialog = d
		return m, nil
	case commentsMsg:
		delete(m.pending, "comments:"+x.key)
		if x.err != nil {
			m.notice(x.err.Error(), true)
			return m, nil
		}
		if r, ok := m.details[x.key]; ok {
			r.issue.Fields.Comments = jira.Comments{Comments: x.comments, Total: len(x.comments)}
			m.details[x.key] = r
			_ = m.cache.Put("detail:"+x.key, r.issue)
			m.detailFor = ""
		}
		m.notice("All comments loaded", false)
		return m, nil
	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		if m.loading || m.mutating || len(m.pending) > 0 {
			return m, cmd
		}
		return m, nil
	}
	if m.dialog != nil {
		return m.updateDialog(msg)
	}
	if m.searching {
		return m.updateSearch(msg)
	}
	switch x := msg.(type) {
	case tea.MouseMsg:
		return m.updateMouse(x)
	case tea.KeyMsg:
		key := x.String()
		if m.help {
			if key == "?" || key == "esc" || key == "q" {
				m.help = false
			}
			return m, nil
		}
		switch key {
		case "ctrl+c", "q":
			if m.mutating {
				m.notice("Update in flight; wait for the result before quitting", true)
				return m, nil
			}
			return m, tea.Quit
		case "?":
			m.help = true
			return m, nil
		case "!":
			if m.lastError != "" {
				m.beginError(m.lastError)
			}
			return m, nil
		case "tab":
			switch m.focus {
			case "list":
				m.focus = "detail"
			case "detail":
				m.focus = "sidebar"
			default:
				m.focus = "list"
			}
			return m, nil
		case "shift+tab":
			switch m.focus {
			case "list":
				m.focus = "sidebar"
			case "sidebar":
				m.focus = "detail"
			default:
				m.focus = "list"
			}
			return m, nil
		case "1", "2", "3", "4", "5":
			n, _ := strconv.Atoi(key)
			return m, m.switchView(n - 1)
		case "b":
			m.boardMode = !m.boardMode
			m.focus = "list"
			return m, nil
		case "/":
			m.searching = true
			m.local = ""
			m.searchInput.SetValue(m.search)
			m.searchInput.Focus()
			return m, textinput.Blink
		case ":":
			m.beginInput("jql", "Raw JQL · overrides structured filters", m.query())
			return m, textinput.Blink
		case "f":
			m.beginFilters()
			return m, textinput.Blink
		case "ctrl+s":
			m.beginInput("save", "Save this query as a named filter", "")
			return m, textinput.Blink
		case "g":
			m.loading = true
			client, project := m.client, m.cfg.Filters.Project
			return m, tea.Batch(func() tea.Msg { b, err := client.Boards(project); return boardsMsg{b, err} }, m.spinner.Tick)
		case "r":
			key := m.detailKey
			cmd := m.reload(false)
			if key != "" {
				return m, tea.Batch(cmd, m.detail(key, true))
			}
			return m, cmd
		case "o":
			m.beginInput("open", "Open ticket key internally", "")
			return m, textinput.Blink
		case "O":
			i, ok := m.activeIssue()
			if ok {
				if r, yes := m.details[i.Key]; yes {
					i = r.issue
				}
				if u := jira.WebURL(i); u != "" {
					return m, openURL(u)
				}
				m.notice("Open ticket details first to obtain its Jira URL", true)
			}
			return m, nil
		case "l":
			if m.boardMode && m.focus == "list" {
				m.boardMove(1, 0)
				return m, m.selection()
			}
			i, ok := m.activeIssue()
			if ok {
				if r, yes := m.details[i.Key]; yes {
					i = r.issue
				}
				m.beginLinks(i)
			}
			return m, nil
		case "a", "t", "c", "e", "L", "m":
			if m.readOnly {
				m.notice("Read-only mode: ticket updates are disabled", true)
				return m, nil
			}
			if m.mutating {
				m.notice("Another update is in flight", true)
				return m, nil
			}
			return m, m.beginAction(key)
		case "C":
			key := m.detailKey
			if key == "" {
				return m, nil
			}
			if m.pending["comments:"+key] {
				return m, nil
			}
			m.pending["comments:"+key] = true
			client := m.client
			return m, tea.Batch(func() tea.Msg { c, err := client.Comments(key); return commentsMsg{key, c, err} }, m.spinner.Tick)
		case "esc", "backspace":
			if len(m.history) > 0 {
				key := m.history[len(m.history)-1]
				m.history = m.history[:len(m.history)-1]
				m.detailKey = key
				m.detailFor = ""
				m.viewport.GotoTop()
				return m, m.detail(key, false)
			}
			m.focus = "list"
			return m, nil
		}
		if m.focus == "sidebar" {
			switch key {
			case "j", "down":
				m.sidebar = min(m.sidebar+1, 4+len(m.cfg.Saved))
			case "k", "up":
				m.sidebar = max(m.sidebar-1, 0)
			case "enter":
				return m, m.switchView(m.sidebar)
			}
			return m, nil
		}
		if m.focus == "detail" {
			if key == "enter" {
				return m, nil
			}
			var cmd tea.Cmd
			m.viewport, cmd = m.viewport.Update(x)
			return m, cmd
		}
		switch key {
		case "enter":
			i, ok := m.selectedIssue()
			if ok {
				m.detailKey = i.Key
				m.focus = "detail"
				m.history = nil
				m.detailFor = ""
				m.viewport.GotoTop()
				return m, m.detail(i.Key, false)
			}
		case "j", "down":
			if m.boardMode {
				m.boardMove(0, 1)
			} else {
				m.selected = min(m.selected+1, len(m.visible)-1)
			}
			return m, m.selection()
		case "k", "up":
			if m.boardMode {
				m.boardMove(0, -1)
			} else {
				m.selected = max(m.selected-1, 0)
			}
			return m, m.selection()
		case "h", "left":
			if m.boardMode {
				m.boardMove(-1, 0)
				return m, m.selection()
			}
		case "l", "right":
			if m.boardMode {
				m.boardMove(1, 0)
				return m, m.selection()
			}
		case "pgdown":
			m.selected = min(m.selected+max(1, (m.height-8)/2), len(m.visible)-1)
			return m, m.selection()
		case "pgup":
			m.selected = max(m.selected-max(1, (m.height-8)/2), 0)
			return m, m.selection()
		case "home":
			m.selected = 0
			return m, m.selection()
		case "end":
			m.selected = max(0, len(m.visible)-1)
			return m, m.selection()
		}
	}
	return m, nil
}
func (m *Model) updateSearch(msg tea.Msg) (tea.Model, tea.Cmd) {
	if key, ok := msg.(tea.KeyMsg); ok {
		switch key.String() {
		case "esc", "ctrl+c":
			m.searching = false
			m.local = ""
			m.rebuildVisible()
			return m, m.selection()
		case "enter":
			m.searching = false
			m.search = m.searchInput.Value()
			m.local = ""
			m.raw = ""
			return m, m.reload(true)
		}
	}
	var cmd tea.Cmd
	m.searchInput, cmd = m.searchInput.Update(msg)
	m.local = m.searchInput.Value()
	m.selected = 0
	m.rebuildVisible()
	return m, cmd
}
func (m *Model) statusGroups() ([]string, map[string][]int) {
	groups := map[string][]int{}
	categories := map[string]string{}
	for pos, n := range m.visible {
		i := m.items[n]
		name := jira.SafeLine(i.Fields.Status.Name)
		if name == "" {
			name = "Unknown"
		}
		groups[name] = append(groups[name], pos)
		categories[name] = i.Fields.Status.Category.Key
	}
	var names []string
	for name := range groups {
		names = append(names, name)
	}
	ranks := map[string]int{"new": 0, "indeterminate": 1, "done": 2}
	sort.Slice(names, func(i, j int) bool {
		a, b := ranks[categories[names[i]]], ranks[categories[names[j]]]
		if a != b {
			return a < b
		}
		return names[i] < names[j]
	})
	return names, groups
}
func (m *Model) boardMove(horizontal, vertical int) {
	names, groups := m.statusGroups()
	if len(names) == 0 {
		return
	}
	i, ok := m.selectedIssue()
	if !ok {
		return
	}
	col, row := 0, 0
	for c, name := range names {
		for r, pos := range groups[name] {
			if pos == m.selected {
				col = c
				row = r
			}
		}
	}
	_ = i
	if horizontal != 0 {
		col = max(0, min(len(names)-1, col+horizontal))
	}
	rows := groups[names[col]]
	row = max(0, min(len(rows)-1, row+vertical))
	m.selected = rows[row]
}
