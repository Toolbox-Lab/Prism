use clap::Parser;
use grat_core::taxonomy::loader::TaxonomyDatabase;
use tabled::Table;
use tabled::Tabled;

#[derive(Parser, Debug)]
#[command(
    about = "Search the taxonomy database for specific error names, categories, or descriptions"
)]
pub struct SearchErrorArgs {
    #[arg(help = "The search query (case-insensitive substring match)")]
    pub query: String,
}

#[derive(Tabled)]
struct SearchResultRow {
    #[tabled(rename = "Category")]
    category: String,
    #[tabled(rename = "Code")]
    code: u32,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Summary")]
    summary: String,
}

pub async fn run(args: SearchErrorArgs) -> anyhow::Result<()> {
    let db = TaxonomyDatabase::load_latest()?;

    let results = db.search(&args.query);

    if results.is_empty() {
        println!("No matching entries found for '{}'", args.query);
        return Ok(());
    }

    let rows: Vec<SearchResultRow> = results
        .into_iter()
        .map(|entry| SearchResultRow {
            category: entry.category.to_string(),
            code: entry.code,
            name: entry.name.clone(),
            summary: entry.summary.clone(),
        })
        .collect();

    let table = Table::new(rows);
    println!("{table}");

    Ok(())
}
