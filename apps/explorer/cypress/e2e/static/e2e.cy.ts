// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

Cypress.config('baseUrl', 'http://localhost:8080');

// Standardized CSS Selectors
const mainBodyCSS = 'main > section > div';
const nftObject = (num: number) => `div#ownedObjects > div:nth-child(${num}) a`;
const ownerButton = 'td#owner > div > div';

// Standardized expectations:
const expectErrorResult = () => {
    cy.get(mainBodyCSS).invoke('attr', 'id').should('eq', 'errorResult');
};

const searchText = (text: string) => {
    // TODO: Ideally this should just call `submit` but the search isn't a form yet:
    cy.get('#searchText').type(text).get('#searchBtn').click();
};

describe('End-to-end Tests', () => {
    describe('Wrong Search', () => {
        it('leads to error page', () => {
            cy.visit('/');
            searchText('apples');
            expectErrorResult();
        });
    });

    describe('Transaction Results', () => {
        const successID = 'Da4vHc9IwbvOYblE8LnrVsqXwryt2Kmms+xnJ7Zx5E4=';
        it('can be searched', () => {
            cy.visit('/');
            searchText(successID);
            cy.get('[data-testid=pageheader]').contains(successID);
        });

        it('can be reached through URL', () => {
            cy.visit(`/transaction/${successID}`);
            cy.get('[data-testid=pageheader]').contains(successID);
        });

        it('includes the sender time information', () => {
            cy.visit(`/transaction/${successID}`);
            // TODO - use the custom command date format function
            cy.get('[data-testid=transaction-timestamp]').contains(
                new Intl.DateTimeFormat('en-US', {
                    month: 'short',
                    day: 'numeric',
                    year: 'numeric',
                    hour: 'numeric',
                    minute: 'numeric',
                }).format(new Date('Dec 15, 2024, 00:00:00 UTC'))
            );
        });
    });

    describe('Owned Objects have links that enable', () => {
        it('going from object to child object and back', () => {
            cy.visit('/object/player2');
            cy.get(nftObject(1)).click();
            cy.get('#objectID').contains('Image1');
            cy.get(ownerButton).click();
            cy.get('#objectID').contains('player2');
        });

        it('going from parent to broken image object and back', () => {
            const parentValue = 'ObjectWBrokenChild';
            cy.visit(`/object/${parentValue}`);
            cy.get(nftObject(1)).click();
            cy.get('#noImage');
            cy.get(ownerButton).click();
            cy.get('#loadedImage');
        });
    });


    describe('Group View', () => {
        it('evaluates balance', () => {
            const address = 'ownsAllAddress';
            const rowCSSSelector = (row: number) =>
                `#groupCollection [data-testid=ownedcoinsummary]:nth-child(${row}) `;
            const label = '[data-testid=ownedcoinlabel]';
            const count = '[data-testid=ownedcoinobjcount]';
            const balance = '[data-testid=ownedcoinbalance]';

            cy.visit(`/address/${address}`);

            cy.get(`${rowCSSSelector(1)} ${label}`).contains('USD');
            cy.get(`${rowCSSSelector(1)} ${count}`).contains('2');
            cy.get(`${rowCSSSelector(1)} ${balance}`).contains(
                '9,007,199,254,740,993'
            );

            cy.get(`${rowCSSSelector(2)} ${label}`).contains('SUI');
            cy.get(`${rowCSSSelector(2)} ${count}`).contains('2');
            cy.get(`${rowCSSSelector(2)} ${balance}`).contains('0.0000002');
        });
    });

    // TODO: This test isn't great, ideally we'd either do some more manual assertions, validate linking,
    // or use visual regression testing.
    describe('Transactions for ID', () => {
        it('are displayed from and to address', () => {
            const address = 'ownsAllAddress';
            cy.visit(`/address/${address}`);

            cy.get('[data-testid=tx] td').should('have.length.greaterThan', 0);
        });

        it('are displayed for input and mutated object', () => {
            const address = 'CollectionObject';
            cy.visit(`/address/${address}`);

            cy.get('[data-testid=tx] td').should('have.length.greaterThan', 0);
        });
    });
});
