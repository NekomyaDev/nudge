# 22/60 - Comments and documentation

Use `//` for source comments. A `///` line is lexed as a regular line
comment today, so it is a useful convention for public API documentation but
is not yet extracted into generated docs. Keep comments focused on intent and
use Markdown in your repository documentation for longer guides.

Try:
  nudgec check comments_docs.ndg
