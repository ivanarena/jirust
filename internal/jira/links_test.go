package jira

import "testing"

func TestSameSiteBrowseLinksAreInternal(t *testing.T) {
	i := Issue{Key: "DEV-1", Self: "https://example.atlassian.net/rest/api/3/issue/1"}
	target := URLTarget(i, "https://example.atlassian.net/browse/DEV-2?source=link")
	if target.Key != "DEV-2" || target.URL != "" {
		t.Fatal(target)
	}
	target = URLTarget(i, "https://another.atlassian.net/browse/DEV-2")
	if target.Key != "" || target.URL == "" {
		t.Fatal("cross-site link interpreted internally")
	}
	if q := Query("sprint", Filters{Sprint: "123"}, "", ""); q != "sprint = 123 ORDER BY updated DESC" {
		t.Fatal(q)
	}
}
