//! Publication across the relational store and local settings. Never install
//! candidate memory or notify observers before this boundary returns success.
pub trait Publication {
    type Error;
    fn write(&self) -> Result<(), Self::Error>;
    fn project(&self) -> Result<(), Self::Error>;
    fn restore(&self) -> Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum PublicationFailure<E> {
    Write(E),
    Projection { error: E, recovery: Result<(), E> },
}
pub fn publish<P: Publication>(publication: &P) -> Result<(), PublicationFailure<P::Error>> {
    publication.write().map_err(PublicationFailure::Write)?;
    if let Err(error) = publication.project() {
        return Err(PublicationFailure::Projection {
            error,
            recovery: publication.restore(),
        });
    }
    Ok(())
}
