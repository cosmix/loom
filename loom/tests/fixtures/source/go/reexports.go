package source

// Go has no TS/Python-style re-export syntax; this is an aliased import.
import y "example.com/project/pkg"

func Build() {
	_ = y.Value
}
