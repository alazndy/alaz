type Action = (form: FormData) => Promise<void>;
type Props = { emails: string[]; error: boolean; actions: { add: Action; remove: Action } };

export function MembersView({ emails, error, actions }: Props) {
  return (
    <main>
      <a href="/reports">← Raporlar</a>
      <h1>Davetliler</h1>
      <p className="muted">Bu listedeki kişiler bütün raporları okur. Her biri e-postasına gelen kodla girer.</p>
      <form className="filters" action={actions.add}>
        <label>
          E-posta
          <input type="email" name="email" required maxLength={254} />
        </label>
        <button type="submit">Davet et</button>
      </form>
      {error && <p className="danger">Bu geçerli bir e-posta adresi değil.</p>}
      {emails.length === 0 && <p>Henüz kimse davetli değil.</p>}
      <ul>
        {emails.map((email) => (
          <li key={email}>
            <form className="member" action={actions.remove}>
              <span>{email}</span>
              <input type="hidden" name="email" value={email} />
              <button type="submit">Kaldır</button>
            </form>
          </li>
        ))}
      </ul>
    </main>
  );
}
