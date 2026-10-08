package cache

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestPrivateAccountScopedCache(t *testing.T) {
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	a, err := New("account-a:FAKE_CREDENTIAL_FOR_TEST", false)
	if err != nil {
		t.Fatal(err)
	}
	b, err := New("account-b", false)
	if err != nil {
		t.Fatal(err)
	}
	if a.Dir == b.Dir || strings.Contains(a.Dir, "FAKE_CREDENTIAL") {
		t.Fatal("scope not hashed")
	}
	if err = a.Put("query with private names", []string{"DEV-1"}); err != nil {
		t.Fatal(err)
	}
	var items []string
	stamp, ok := a.Get("query with private names", &items)
	if !ok || stamp.IsZero() || len(items) != 1 {
		t.Fatal(items)
	}
	if _, ok = b.Get("query with private names", &items); ok {
		t.Fatal("cross-account cache hit")
	}
	entries, _ := os.ReadDir(a.Dir)
	for _, entry := range entries {
		p := filepath.Join(a.Dir, entry.Name())
		info, _ := os.Stat(p)
		if info.Mode().Perm() != 0600 {
			t.Fatal(info.Mode())
		}
		data, _ := os.ReadFile(p)
		if strings.Contains(string(data), "FAKE_CREDENTIAL") {
			t.Fatal("credential stored")
		}
	}
	a.Remove("query with private names")
	if _, ok = a.Get("query with private names", &items); ok {
		t.Fatal("cache not removed")
	}
}
func TestDisabledAndCorruptCache(t *testing.T) {
	s, _ := New("anything", true)
	if err := s.Put("key", 123); err != nil {
		t.Fatal(err)
	}
	var value int
	if _, ok := s.Get("key", &value); ok {
		t.Fatal("disabled hit")
	}
	t.Setenv("XDG_CACHE_HOME", t.TempDir())
	s, _ = New("account", false)
	_ = os.WriteFile(filepath.Join(s.Dir, hash("key")+".json"), []byte("bad JSON"), 0600)
	if _, ok := s.Get("key", &value); ok {
		t.Fatal("corrupt hit")
	}
}
