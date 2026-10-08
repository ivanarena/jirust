// Package cache stores private, account-scoped Jira data locally, never credentials.
package cache

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"time"
)

type Store struct {
	Dir      string
	Disabled bool
}
type Entry struct {
	Updated time.Time       `json:"updated"`
	Data    json.RawMessage `json:"data"`
}

func New(scope string, disabled bool) (*Store, error) {
	if disabled {
		return &Store{Disabled: true}, nil
	}
	root, err := os.UserCacheDir()
	if err != nil {
		return nil, err
	}
	s := &Store{Dir: filepath.Join(root, "jiraui", hash(scope))}
	if err = os.MkdirAll(s.Dir, 0700); err != nil {
		return nil, err
	}
	return s, nil
}
func hash(s string) string { sum := sha256.Sum256([]byte(s)); return hex.EncodeToString(sum[:]) }
func (s *Store) Get(key string, dest any) (time.Time, bool) {
	if s == nil || s.Disabled {
		return time.Time{}, false
	}
	b, err := os.ReadFile(filepath.Join(s.Dir, hash(key)+".json"))
	if err != nil {
		return time.Time{}, false
	}
	var e Entry
	if json.Unmarshal(b, &e) != nil || json.Unmarshal(e.Data, dest) != nil {
		return time.Time{}, false
	}
	return e.Updated, true
}
func (s *Store) Put(key string, data any) error {
	if s == nil || s.Disabled {
		return nil
	}
	raw, err := json.Marshal(data)
	if err != nil {
		return err
	}
	b, err := json.Marshal(Entry{Updated: time.Now(), Data: raw})
	if err != nil {
		return err
	}
	f, err := os.CreateTemp(s.Dir, ".write-*")
	if err != nil {
		return err
	}
	name := f.Name()
	defer os.Remove(name)
	if _, err = f.Write(b); err != nil {
		f.Close()
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	return os.Rename(name, filepath.Join(s.Dir, hash(key)+".json"))
}
func (s *Store) Remove(key string) {
	if s != nil && !s.Disabled {
		_ = os.Remove(filepath.Join(s.Dir, hash(key)+".json"))
	}
}
