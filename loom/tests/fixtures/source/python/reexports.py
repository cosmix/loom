from .mod import thing as public_thing
from .runtime import run as aliased_run

__all__ = ["public_thing"]


def build():
    return aliased_run()
