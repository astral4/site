//! Code for building complete HTML pages from article bodies.

use crate::{css::Font, OUTPUT_SITE_CSS_FILE_ABSOLUTE};
use anyhow::{bail, Error, Result};
use ego_tree::{tree, NodeId, NodeMut, Tree};
use jiff::civil::Date;
use markup5ever::{interface::QuirksMode, namespace_url, ns, Attribute, LocalName, QualName};
use scraper::{
    node::{Doctype, Element, Node, Text},
    Html,
};

const OUTPUT_KATEX_CSS_FILE: &str = "/stylesheets/katex.css";

pub struct PageBuilder {
    html: Tree<Node>,
    head_id: NodeId,
    slot_id: NodeId,
}

impl PageBuilder {
    /// Initializes a webpage HTML builder. Every page built:
    /// - includes the provided author as a metadata tag
    /// - specifies preloaded fonts based on the provided list of font sources
    /// - has a `<body>` structure based on the provided template
    ///
    /// # Errors
    /// This function returns an error if:
    /// - the input template cannot be successfully parsed as no-quirks HTML
    /// - the input template does not contain a `<main>` element for slotting page content
    pub fn new(author: &str, site_fonts: &[Font], template: &str) -> Result<Self> {
        // Parse template into tree of HTML nodes
        let template = parse_html(template)?;

        let mut html = Html::new_document();
        let mut root_node = html.tree.root_mut();

        // Add `<!DOCTYPE html>`
        root_node.append(Node::Doctype(Doctype {
            name: "html".into(),
            public_id: "".into(),
            system_id: "".into(),
        }));

        // Add `<html lang="en">`
        let mut html_el_node = root_node.append(create_el_with_attrs("html", &[("lang", "en")]));

        // Add `<head>` within `<html>`
        let mut head_el_node = html_el_node.append_subtree(tree! {
            create_el("head") => {
                create_el_with_attrs("meta", &[("charset", "utf-8")]),
                create_el_with_attrs("meta", &[("name", "viewport"), ("content", "width=device-width, initial-scale=1")]),
                create_el_with_attrs("meta", &[("name", "author"), ("content", author)]),
                create_el_with_attrs("link", &[("rel", "stylesheet"), ("href", OUTPUT_SITE_CSS_FILE_ABSOLUTE)])
            }
        });

        // Add font `<link>`s within `<head>`
        for font in site_fonts {
            let mut attrs = Vec::with_capacity(5);
            attrs.push(("rel", "preload"));
            attrs.push(("href", &font.path));
            attrs.push(("as", "font"));
            // Preloaded fonts need to have a "crossorigin" attribute set to "anonymous"
            // even when the source is not cross-origin.
            // https://developer.mozilla.org/en-US/docs/Web/HTML/Attributes/rel/preload#cors-enabled_fetches
            attrs.push(("crossorigin", "anonymous"));

            if let Some(mime) = font.mime {
                attrs.push(("type", mime));
            }

            head_el_node.append(create_el_with_attrs("link", &attrs));
        }

        let head_id = head_el_node.id();

        // Add `<body>` within `<html>`
        let mut body_el_node = html_el_node.append(create_el("body"));

        // Add template within `<body>`
        append_fragment(&mut body_el_node, template);

        // Find element in template for slotting page content
        // We search in reverse insertion order because the template's HTML nodes were inserted last.
        let Some(slot_id) = html.tree.nodes().rev().find_map(|node| {
            node.value()
                .as_element()
                .is_some_and(|el| el.name() == "main") // "We have components at home"
                .then(|| node.id())
        }) else {
            bail!("template does not have a `<main>` element for slotting page content");
        };

        Ok(Self {
            html: html.tree,
            head_id,
            slot_id,
        })
    }

    /// Outputs a string containing a complete HTML document based on the provided document title and body
    /// (and article metadata if the page is an article).
    ///
    /// # Errors
    /// This function returns an error if the input body cannot be successfully parsed as no-quirks HTML.
    pub fn build_page(&self, title: &str, body: &str, kind: PageKind) -> Result<String> {
        let body = parse_html(body)?;
        Ok(self.build_page_inner(title, body, kind))
    }

    fn build_page_inner(&self, title: &str, body: Tree<Node>, kind: PageKind) -> String {
        let mut html = self.html.clone();

        // Add `<title>` within `<head>`
        // SAFETY: The ID is valid because it was generated in the constructor `PageBuilder::new()`.
        let mut head_node = unsafe { html.get_unchecked_mut(self.head_id) };

        if contains_math(&body, kind) {
            head_node.append(create_el_with_attrs(
                "link",
                &[("rel", "stylesheet"), ("href", OUTPUT_KATEX_CSS_FILE)],
            ));
        }

        head_node.append_subtree(tree! {
            create_el("title") => { create_text(title) }
        });

        // Add page content within template slot
        // SAFETY: The ID is valid because it was generated in the constructor `PageBuilder::new()`.
        let mut slot_node = unsafe { html.get_unchecked_mut(self.slot_id) };
        let mut slot_node = match kind {
            PageKind::Fragment => slot_node,
            PageKind::Article { .. } => slot_node.append(create_el("article")),
        };

        // Add heading section with title and created/last-updated dates for article pages
        if let PageKind::Article {
            created, updated, ..
        } = kind
        {
            let created_date_string = created.to_string();

            let mut heading_section_tree = tree! {
                Node::Fragment => { Node::Fragment => {
                    create_el_with_attrs("hgroup", &[("class", "__article-heading")]) => {
                        create_el("h1") => {
                            create_text(title)
                        },
                        create_el("p") => {
                            create_el_with_attrs("time", &[("datetime", &created_date_string)]) => {
                                create_text(&created_date_string)
                            }
                        },
                    }
                }}
            };

            // Add last-updated date if it exists
            if let Some(updated) = updated {
                // Find the root node of the dates section (`<p>`)
                let date_section_root_id = heading_section_tree
                    .nodes()
                    .find(|node| node.value().as_element().is_some_and(|el| el.name() == "p"))
                    .unwrap()
                    .id();

                // SAFETY: Indexing is guaranteed to be valid because the ID was obtained from searching the tree nodes.
                let mut date_section_root =
                    unsafe { heading_section_tree.get_unchecked_mut(date_section_root_id) };

                let updated_date_string = updated.to_string();

                // Add last-updated date
                date_section_root.append_subtree(tree! {
                    Node::Fragment => {
                        create_text(" (last updated "),
                        create_el_with_attrs("time", &[("datetime", &updated_date_string)]) => {
                            create_text(&updated_date_string)
                        },
                        create_text(")"),
                    }
                });
            }

            append_fragment(&mut slot_node, heading_section_tree);
        }

        append_fragment(&mut slot_node, body);

        // Serialize document tree
        tree_to_html(html)
    }
}

#[derive(Clone, Copy)]
pub enum PageKind {
    Fragment,
    Article {
        contains_math: bool,
        created: Date,
        updated: Option<Date>,
    },
}

/// Returns an `<img>` element with the provided attributes as a string of HTML.
pub(crate) fn create_img_html(attrs: &[(&str, &str)]) -> String {
    tree_to_html(Tree::new(create_el_with_attrs("img", attrs)))
}

pub struct ArchiveBuilder(Vec<ArticlePreview>);

struct ArticlePreview {
    title: Box<str>,
    slug: Box<str>,
    created: Date,
}

impl ArchiveBuilder {
    /// Initializes a writing archive page builder.
    /// The page includes a list of all articles in reverse chronological order.
    #[must_use]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Adds an article's metadata (title, slug, and creation date) to the builder.
    pub fn add_article(&mut self, title: Box<str>, slug: Box<str>, created: Date) {
        self.0.push(ArticlePreview {
            title,
            slug,
            created,
        });
    }

    /// Consumes the builder, outputting a string containing a complete HTML document for the archive page.
    pub fn into_html(mut self, builder: &PageBuilder) -> String {
        const TITLE: &str = "Writing";

        // Add heading section with title and page description
        let mut html = Tree::new(Node::Fragment);

        let mut root_node = html.root_mut();
        let mut root_node = root_node.append_subtree(tree! {
            Node::Fragment => {
                create_el("h1") => { create_text(TITLE) },
                create_el("p") => { create_text("Posts are in reverse chronological order.") },
            }
        });

        // Sort articles by creation date in reverse chronological order, then by title in reverse lexicographical order
        self.0
            .sort_unstable_by(|a, b| b.created.cmp(&a.created).then(b.title.cmp(&a.title)));

        // Add list of articles
        // We add `role="list"` to `<ol>` because of https://bugs.webkit.org/show_bug.cgi?id=170179
        let mut list_node = root_node.append(create_el_with_attrs(
            "ol",
            &[
                ("reversed", ""),
                ("class", "__article-list"),
                ("role", "list"),
            ],
        ));

        for article in self.0 {
            let date_string = article.created.to_string();

            list_node.append_subtree(tree! {
                create_el("li") => {
                    create_el_with_attrs("p", &[("class", "__article-date")]) => {
                        create_el_with_attrs("time", &[("datetime", &date_string)]) => { create_text(&date_string) }
                    },
                    create_el_with_attrs("a", &[("href", &article.slug)]) => {
                        create_el("p") => { create_text(&article.title) }
                    }
                }
            });
        }

        builder.build_page_inner(TITLE, html, PageKind::Fragment)
    }
}

fn parse_html(input: &str) -> Result<Tree<Node>> {
    let html = Html::parse_fragment(input);

    // `Html::parse_fragment()` does not return a `Result` because
    // the parser is supposed to be resilient and fall back to HTML quirks mode upon encountering errors.
    // So, after parsing, we have to check for any errors encountered ourselves.
    match html.errors.first() {
        Some(err) => Err(Error::msg(err.clone()).context("failed to parse input as valid HTML")),
        None => Ok(html.tree),
    }
}

fn contains_math(html: &Tree<Node>, kind: PageKind) -> bool {
    match kind {
        PageKind::Fragment => {
            html.values().any(|node| {
                node.as_element().is_some_and(|el| {
                    (el.name() == "span" && el.classes().any(|c| c == "katex")) // element is `<span class="katex">`
                        || el.name.ns == ns!(mathml) // element is MathML
                })
            })
        }
        PageKind::Article { contains_math, .. } => contains_math,
    }
}

fn create_el(name: &str) -> Node {
    Node::Element(Element::new(create_name(name, NameKind::Element), vec![]))
}

fn create_el_with_attrs(name: &str, attrs: &[(&str, &str)]) -> Node {
    let attrs = attrs
        .iter()
        .map(|(key, value)| Attribute {
            name: create_name(key, NameKind::Attr),
            value: (*value).into(),
        })
        .collect();

    Node::Element(Element::new(create_name(name, NameKind::Element), attrs))
}

fn create_name(name: &str, kind: NameKind) -> QualName {
    QualName {
        prefix: None,
        ns: match kind {
            NameKind::Element => ns!(html),
            NameKind::Attr => ns!(),
        },
        local: LocalName::try_static(name)
            .expect("calls to this function should supply valid names"),
    }
}

#[derive(Clone, Copy)]
enum NameKind {
    Element,
    Attr,
}

fn create_text(text: &str) -> Node {
    Node::Text(Text { text: text.into() })
}

/// Appends the contents of `fragment` as children of the input `node`.
fn append_fragment(node: &mut NodeMut<'_, Node>, fragment_tree: Tree<Node>) {
    // Fragments have the following structure:
    // Node::Fragment -> { Node::Element("html") -> { <contents> }}
    // After appending the fragment's tree, we have to make the contents direct children of the node.
    let mut fragment_root_node = node.append_subtree(fragment_tree);
    let fragment_root_id = fragment_root_node.id();
    let fragment_html_id = fragment_root_node.first_child().unwrap().id();
    node.reparent_from_id_append(fragment_html_id);
    // SAFETY: Indexing is guaranteed to be valid because
    // the ID was obtained from appending the fragment as a subtree of a node from the tree.
    unsafe { node.tree().get_unchecked_mut(fragment_root_id) }.detach();
}

/// Serializes a tree of HTML nodes as a string of HTML.
fn tree_to_html(tree: Tree<Node>) -> String {
    Html {
        errors: Vec::new(),
        quirks_mode: QuirksMode::NoQuirks,
        tree,
    }
    .html()
}

#[cfg(test)]
mod test {
    use super::{contains_math, create_el, create_el_with_attrs, parse_html, PageKind};
    use jiff::civil::Date;
    use scraper::{Html, Node};

    #[test]
    fn contains_math_markup() {
        /// Utility function for converting a string of HTML to a tree of HTML nodes
        fn html_contains_math(html: &str, kind: PageKind, expected: bool) {
            assert_eq!(contains_math(&parse_html(html).unwrap(), kind), expected);
        }

        html_contains_math(r#"<div class="katex"></div>"#, PageKind::Fragment, false);
        html_contains_math(r#"<span class="k"></span>"#, PageKind::Fragment, false);
        html_contains_math(r#"<span class="katex"></span>"#, PageKind::Fragment, true);
        html_contains_math("<math></math>", PageKind::Fragment, true);
        html_contains_math(
            "<math></math>",
            PageKind::Article {
                contains_math: false,
                created: Date::default(),
                updated: Option::default(),
            },
            false,
        );
        html_contains_math(
            "<div></div>",
            PageKind::Article {
                contains_math: true,
                created: Date::default(),
                updated: Option::default(),
            },
            true,
        );
    }

    /// Utility function for asserting that the HTML representation of `element` is equal to `expected`
    fn assert_eq_serialized(element: Node, expected: &str) {
        let mut html = Html::new_fragment();
        html.tree.root_mut().append(element);
        assert_eq!(html.html(), expected);
    }

    #[test]
    fn create_element() {
        // Non-void element
        assert_eq_serialized(create_el("p"), "<p></p>");

        // Void element
        assert_eq_serialized(create_el("img"), "<img>");
    }

    #[test]
    fn create_element_with_attrs() {
        // Non-void element with single attribute
        assert_eq_serialized(
            create_el_with_attrs("p", &[("id", "abc")]),
            "<p id=\"abc\"></p>",
        );

        // Non-void element with multiple attributes
        assert_eq_serialized(
            create_el_with_attrs("p", &[("id", "abc"), ("class", "def")]),
            "<p id=\"abc\" class=\"def\"></p>",
        );

        // Void element with single attribute
        assert_eq_serialized(
            create_el_with_attrs("img", &[("id", "abc")]),
            "<img id=\"abc\">",
        );

        // Void element with multiple attributes
        assert_eq_serialized(
            create_el_with_attrs("img", &[("id", "abc"), ("class", "def")]),
            "<img id=\"abc\" class=\"def\">",
        );
    }

    #[test]
    fn create_element_with_empty_attrs() {
        // Element with empty attribute name and value
        assert_eq_serialized(create_el_with_attrs("p", &[("", "")]), "<p =\"\"></p>");

        // Element with empty attribute name
        assert_eq_serialized(
            create_el_with_attrs("p", &[("", "abc")]),
            "<p =\"abc\"></p>",
        );

        // Element with empty attribute value
        assert_eq_serialized(create_el_with_attrs("p", &[("id", "")]), "<p id=\"\"></p>");
    }

    #[test]
    #[should_panic]
    fn create_nonexistent_element() {
        // "_" should be an invalid element name
        create_el("_");
    }

    #[test]
    #[should_panic]
    fn create_element_with_nonexistent_attrs() {
        // "_" should be an invalid attribute name
        create_el_with_attrs("p", &[("_", "abc")]);
    }
}
