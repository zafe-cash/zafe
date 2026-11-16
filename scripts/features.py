import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from featuremap import cli

if __name__ == "__main__":
    sys.exit(cli.main(sys.argv[1:]))
