package ui

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/ivanarena/jiraui/internal/jira"
)

var (
	fg            = lipgloss.AdaptiveColor{Light: "#30343B", Dark: "#D8DEE9"}
	muted         = lipgloss.AdaptiveColor{Light: "#6B7280", Dark: "#8290A6"}
	accent        = lipgloss.AdaptiveColor{Light: "#315EA8", Dark: "#89B4FA"}
	border        = lipgloss.AdaptiveColor{Light: "#C7CDD5", Dark: "#414B5B"}
	selectedBG    = lipgloss.AdaptiveColor{Light: "#E3EBF8", Dark: "#26374F"}
	titleStyle    = lipgloss.NewStyle().Foreground(accent).Bold(true)
	textStyle     = lipgloss.NewStyle().Foreground(fg)
	mutedStyle    = lipgloss.NewStyle().Foreground(muted)
	selectedStyle = lipgloss.NewStyle().Foreground(fg).Background(selectedBG).Bold(true)
	errorStyle    = lipgloss.NewStyle().Foreground(lipgloss.AdaptiveColor{Light: "#A52A36", Dark: "#F38BA8"})
)

func truncate(s string, width int) string {
	if width <= 0 {
		return ""
	}
	return ansi.Truncate(s, width, "…")
}
func wrap(s string, width int) string { return ansi.Wrap(jira.Safe(s), max(1, width), "") }
func pad(s string, width int) string {
	return truncate(s, width) + strings.Repeat(" ", max(0, width-lipgloss.Width(truncate(s, width))))
}
func panel(title, body string, w, h int, active bool) string {
	iw, ih := max(1, w-2), max(1, h-2)
	rows := append([]string{titleStyle.Render(truncate(title, iw))}, strings.Split(body, "\n")...)
	rows = rows[:min(len(rows), ih)]
	for n, row := range rows {
		rows[n] = truncate(row, iw)
	}
	color := border
	if active {
		color = accent
	}
	return lipgloss.NewStyle().Width(iw).Height(ih).Border(lipgloss.RoundedBorder()).BorderForeground(color).Render(strings.Join(rows, "\n"))
}
func (m Model) widths() (int, int, int) {
	side := min(24, max(18, m.width/6))
	rest := max(1, m.width-side)
	if m.boardMode && m.focus != "detail" {
		return side, rest, 0
	}
	if m.width < 110 {
		if m.focus == "detail" {
			return side, 0, rest
		}
		return side, rest, 0
	}
	detail := max(34, rest*45/100)
	return side, rest - detail, detail
}
func (m Model) sidebarText(width int) string {
	options := []string{"1  My work", "2  All tickets", "3  Backlog", "4  Active sprint", "5  Status board"}
	for _, p := range m.cfg.Saved {
		options = append(options, "   "+jira.SafeLine(p.Name))
	}
	var rows []string
	for n, s := range options {
		s = truncate(s, width-2)
		if n == m.sidebar {
			rows = append(rows, selectedStyle.Render("› "+s))
		} else {
			rows = append(rows, "  "+s)
		}
	}
	rows = append(rows, "", mutedStyle.Render("FILTERS"))
	f := m.cfg.Filters
	for _, p := range [][2]string{{"Project", f.Project}, {"Assignee", f.Assignee}, {"Status", f.Status}, {"Reporter", f.Reporter}, {"Type", f.Type}, {"Priority", f.Priority}, {"Labels", f.Labels}, {"Sprint", f.Sprint}} {
		if p[1] != "" {
			rows = append(rows, truncate(p[0]+": "+jira.SafeLine(p[1]), width))
		}
	}
	if m.sprintName != "" {
		rows = append(rows, "", truncate("Sprint: "+m.sprintName, width))
	}
	rows = append(rows, "", mutedStyle.Render("f filters · : JQL"), mutedStyle.Render("g boards/sprints"), mutedStyle.Render("Ctrl+S save query"))
	if m.raw != "" {
		rows = append(rows, "", mutedStyle.Render("Raw JQL overrides"), mutedStyle.Render("structured filters"))
	}
	return strings.Join(rows, "\n")
}
func (m *Model) listText(w, h int) string {
	if len(m.visible) == 0 {
		if m.loading {
			return mutedStyle.Render("Loading…")
		}
		return "No matching loaded tickets.\n\n/ search Jira · f filters · 2 all\n: edit JQL · r refresh"
	}
	rows := max(1, (h-4)/2)
	if m.selected < m.listOffset {
		m.listOffset = m.selected
	}
	if m.selected >= m.listOffset+rows {
		m.listOffset = m.selected - rows + 1
	}
	m.listOffset = max(0, min(m.listOffset, max(0, len(m.visible)-rows)))
	var out []string
	out = append(out, mutedStyle.Render(truncate("KEY · STATUS · SUMMARY / ASSIGNEE", w-2)))
	for p := m.listOffset; p < min(len(m.visible), m.listOffset+rows); p++ {
		i := m.items[m.visible[p]]
		first := i.Key + " · " + jira.SafeLine(i.Fields.Status.Name) + " · " + jira.SafeLine(i.Fields.Summary)
		second := "    " + i.Fields.Assignee.String() + " · " + jira.SafeLine(i.Fields.Priority.Name)
		if p == m.selected {
			out = append(out, selectedStyle.Render(pad("› "+first, w-2)), selectedStyle.Render(pad(second, w-2)))
		} else {
			out = append(out, truncate("  "+first, w-2), mutedStyle.Render(truncate(second, w-2)))
		}
	}
	return strings.Join(out, "\n")
}
func (m *Model) boardLayout(w, h int) ([]string, map[string][]int, int, map[string]int) {
	names, groups := m.statusGroups()
	if len(names) == 0 {
		return names, groups, w - 2, map[string]int{}
	}
	count := max(1, min(len(names), (w-2)/24))
	col := 0
	for n, name := range names {
		for _, pos := range groups[name] {
			if pos == m.selected {
				col = n
			}
		}
	}
	start := max(0, min(col-count+1, len(names)-count))
	shown := names[start : start+count]
	columnWidth := max(1, (w-2)/count)
	capacity := max(1, (h-4)/3)
	offsets := map[string]int{}
	for _, name := range shown {
		for row, pos := range groups[name] {
			if pos == m.selected {
				offsets[name] = max(0, row-capacity+1)
			}
		}
	}
	return shown, groups, columnWidth, offsets
}
func (m *Model) boardText(w, h int) string {
	names, groups, cw, offsets := m.boardLayout(w, h)
	if len(names) == 0 {
		return "No tickets in the loaded query.\n\nf filters · g choose board/sprint"
	}
	var columns []string
	capacity := max(1, (h-4)/3)
	for _, name := range names {
		rows := []string{titleStyle.Render(pad(fmt.Sprintf("%s (%d)", name, len(groups[name])), cw))}
		for row := 0; row < capacity; row++ {
			n := row + offsets[name]
			if n >= len(groups[name]) {
				rows = append(rows, strings.Repeat(" ", cw), strings.Repeat(" ", cw), strings.Repeat(" ", cw))
				continue
			}
			pos := groups[name][n]
			i := m.items[m.visible[pos]]
			first := pad(i.Key+" "+jira.SafeLine(i.Fields.Summary), cw)
			second := pad(i.Fields.Assignee.String(), cw)
			if pos == m.selected {
				first = selectedStyle.Render(first)
				second = selectedStyle.Render(second)
			} else {
				second = mutedStyle.Render(second)
			}
			rows = append(rows, first, second, strings.Repeat(" ", cw))
		}
		columns = append(columns, strings.Join(rows, "\n"))
	}
	return lipgloss.JoinHorizontal(lipgloss.Top, columns...)
}
func (m *Model) detailText(w, h int) string {
	iw, ih := max(1, w-2), max(1, h-3)
	m.viewport.Width = iw
	m.viewport.Height = ih
	r, ok := m.details[m.detailKey]
	if !ok {
		if m.detailKey == "" {
			return "Select a ticket.\nEnter focuses full details."
		}
		if m.pending[m.detailKey] {
			return "Loading " + m.detailKey + "…"
		}
		return "Details not loaded.\n\nr refresh · o open ticket key"
	}
	stamp := fmt.Sprintf("%s:%d:%d", r.issue.Key, r.updated.UnixNano(), iw)
	if m.detailFor != stamp {
		oldOffset := m.viewport.YOffset
		m.detailLines = nil
		m.lineLinks = map[int]jira.Target{}
		add := func(text string, target *jira.Target) {
			for _, line := range strings.Split(wrap(text, iw), "\n") {
				n := len(m.detailLines)
				m.detailLines = append(m.detailLines, line)
				if target != nil {
					m.lineLinks[n] = *target
				} else if u := jira.URLs.FindString(line); u != "" {
					u = strings.TrimRight(u, ".,;)]}")
					if jira.ValidateURL(u) == nil {
						m.lineLinks[n] = jira.URLTarget(r.issue, u)
					}
				} else if key := jira.IssueKeys.FindString(line); key != "" && key != m.detailKey {
					m.lineLinks[n] = jira.Target{Key: key}
				}
			}
		}
		i, f := r.issue, r.issue.Fields
		add(i.Key+" · "+f.Summary, nil)
		add("", nil)
		for _, line := range []string{"Status: " + f.Status.Name, "Assignee: " + f.Assignee.String(), "Reporter: " + f.Reporter.String(), "Type: " + f.Type.Name + " · Priority: " + f.Priority.Name, "Project: " + f.Project.Key, "Labels: " + strings.Join(f.Labels, ", "), "Created: " + f.Created, "Updated: " + f.Updated} {
			add(line, nil)
		}
		if f.DueDate != "" {
			add("Due: "+f.DueDate, nil)
		}
		add("\nDESCRIPTION", nil)
		body := jira.Text(f.Description)
		if body == "" {
			body = "(no description)"
		}
		add(body, nil)
		if targets := jira.Targets(i); len(targets) > 0 {
			add("\nRELATED TICKETS / LINKS · click or press l", nil)
			for _, t := range targets {
				t := t
				add("↗ "+t.Label, &t)
			}
		}
		if custom := jira.CustomFields(i); len(custom) > 0 {
			add("\nCUSTOM FIELDS", nil)
			for _, line := range custom {
				add(line, nil)
			}
		}
		add(fmt.Sprintf("\nCOMMENTS · loaded %d of %d", len(f.Comments.Comments), f.Comments.Total), nil)
		if f.Comments.Total > len(f.Comments.Comments) {
			add("Press C to load all comments through acli.", nil)
		}
		for _, c := range f.Comments.Comments {
			add("\n"+c.Author.String()+" · "+c.Created, nil)
			add(jira.Text(c.Body), nil)
		}
		m.viewport.SetContent(strings.Join(m.detailLines, "\n"))
		m.viewport.SetYOffset(oldOffset)
		m.detailFor = stamp
	}
	return m.viewport.View()
}

const actionBar = "f Filters  a Assign  t Status  e Edit  c Comment  l Links  O Browser  ? Help"

func (m *Model) View() string {
	if m.width == 0 || m.height == 0 {
		return "Starting jiraui…"
	}
	if m.width < 60 || m.height < 18 {
		return "jiraui needs at least 60 columns × 18 rows.\nResize/zoom this pane. q quits."
	}
	suffix := ""
	if m.readOnly {
		suffix = " · READ ONLY"
	}
	if m.client.Name() == "demo" {
		suffix += " · SYNTHETIC DATA"
	}
	activity := ""
	if m.loading || m.mutating || len(m.pending) > 0 {
		activity = " " + m.spinner.View()
	}
	header := titleStyle.Render(truncate("jiraui · "+m.client.Name()+suffix+activity, m.width)) + "\n"
	if m.searching {
		m.searchInput.Width = m.width - 4
		header += "/ " + m.searchInput.View()
	} else {
		header += mutedStyle.Render(truncate(m.query(), m.width))
	}
	header += "\n"
	h := m.height - 4
	status := textStyle.Render(truncate(m.status, m.width))
	if m.isError {
		status = errorStyle.Render(truncate(m.status, m.width))
	}
	footer := status + "\n" + mutedStyle.Render(truncate(actionBar, m.width))
	if m.help {
		body := helpText
		return header + lipgloss.Place(m.width, h, lipgloss.Center, lipgloss.Center, panel("Jira workspace · keyboard reference", body, min(92, m.width), min(28, h), true)) + "\n" + footer
	}
	if m.dialog != nil {
		body, w, height := m.dialogBody()
		return header + lipgloss.Place(m.width, h, lipgloss.Center, lipgloss.Center, panel("jiraui", body, w, height, true)) + "\n" + footer
	}
	side, list, detail := m.widths()
	panels := []string{panel("WORKSPACE", m.sidebarText(side-2), side, h, m.focus == "sidebar")}
	if list > 0 {
		title := "TICKETS"
		body := m.listText(list, h)
		if m.boardMode {
			title = "STATUS BOARD · loaded query"
			body = m.boardText(list, h)
		}
		panels = append(panels, panel(title, body, list, h, m.focus == "list"))
	}
	if detail > 0 {
		title := "TICKET · Enter focuses · Esc goes back"
		if len(m.history) > 0 {
			title = "TICKET · linked navigation · Esc back"
		}
		panels = append(panels, panel(title, m.detailText(detail, h), detail, h, m.focus == "detail"))
	}
	return header + lipgloss.JoinHorizontal(lipgloss.Top, panels...) + "\n" + footer
}

const helpText = `NAVIGATION
Tab / Shift+Tab       Focus sidebar, tickets, or details
↑↓ / j k             Navigate tickets; scroll focused details
Enter                 Focus full ticket; open selected item
Esc / Backspace       Back from linked ticket; return to list
1–5                   My work / all / backlog / sprint / board
b                     Toggle status board for the current query
g                     Discover boards and their active/future sprints

SEARCH AND FILTERS
/                     Filter loaded tickets; Enter searches Jira
f                     Project, assignee, status, reporter, etc.
:                     Raw JQL (overrides structured filters)
Ctrl+S                Save current query as a sidebar preset
r                     Refresh query and current details

TICKETS AND ACTIONS
o / O                 Open ticket internally / open in browser
l                     Related tickets and external links
C                     Load all comments (acli)
a / t / e / c         Assign / transition / edit / comment
L / m                 Link tickets / move to sprint (jira backend)
Ctrl+S in editor      Review update; y confirms; Esc cancels
Mouse                 Select tickets, click links and footer actions
!                     Read the complete last CLI error
q                     Quit (waits for in-flight updates)

Status board groups loaded tickets, not the web board's columns.
Backlog is a ranked JQL query, not the web board's exact filter.`

func (m *Model) updateMouse(x tea.MouseMsg) (tea.Model, tea.Cmd) {
	side, list, detail := m.widths()
	if x.Button == tea.MouseButtonWheelUp || x.Button == tea.MouseButtonWheelDown {
		if detail > 0 && x.X >= side+list {
			m.focus = "detail"
			var cmd tea.Cmd
			m.viewport, cmd = m.viewport.Update(x)
			return m, cmd
		}
		if x.X >= side {
			step := 1
			if x.Button == tea.MouseButtonWheelUp {
				step = -1
			}
			m.focus = "list"
			m.selected = max(0, min(len(m.visible)-1, m.selected+step))
			return m, m.selection()
		}
		return m, nil
	}
	if x.Button != tea.MouseButtonLeft || x.Action != tea.MouseActionPress {
		return m, nil
	}
	if x.Y == m.height-1 {
		buttons := []struct{ label, key string }{{"f Filters", "f"}, {"a Assign", "a"}, {"t Status", "t"}, {"e Edit", "e"}, {"c Comment", "c"}, {"l Links", "l"}, {"O Browser", "O"}, {"? Help", "?"}}
		for _, b := range buttons {
			start := strings.Index(actionBar, b.label)
			if x.X >= start && x.X < start+len(b.label) {
				return m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(b.key)})
			}
		}
		return m, nil
	}
	if x.Y < 4 || x.Y >= m.height-3 {
		return m, nil
	}
	if x.X < side {
		n := x.Y - 4
		if n <= 4+len(m.cfg.Saved) {
			return m, m.switchView(n)
		}
		return m, nil
	}
	if list > 0 && x.X < side+list {
		m.focus = "list"
		if m.boardMode {
			names, groups, cw, offsets := m.boardLayout(list, m.height-4)
			col := (x.X - side - 1) / max(1, cw)
			row := (x.Y - 5) / 3
			if x.Y >= 5 && col >= 0 && col < len(names) {
				name := names[col]
				row += offsets[name]
				if row >= 0 && row < len(groups[name]) {
					m.selected = groups[name][row]
					return m, m.selection()
				}
			}
		} else {
			pos := m.listOffset + (x.Y-5)/2
			if x.Y >= 5 && pos >= 0 && pos < len(m.visible) {
				m.selected = pos
				return m, m.selection()
			}
		}
		return m, nil
	}
	if detail > 0 {
		m.focus = "detail"
		line := m.viewport.YOffset + x.Y - 4
		if target, ok := m.lineLinks[line]; ok {
			return m, m.openTarget(target)
		}
	}
	return m, nil
}
